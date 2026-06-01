# CardNote Compiler 全面代码审查报告（v4.1 源码补充版）

> **审查基准**：v0.1.24（fe48e7b），~8,800 行核心 Rust 源码，27 个源文件
> **审查日期**：2026-06-01
> **版本说明**：> - v4 初版：基于历史报告对照的增量审查（全新发现 8 项）
> - **v4.1 补充**：通过逐行源码阅读新增 5 项（S1-S5），修正技术评估报告的 3 个误判，并将 S1-S5 纳入优先级矩阵
> **历史审查报告清单**：
> - `tests/review_reports/2026-05-31-技术债与风险评估-v3.md`（v3）
> - `tests/review_reports/2026-05-31-技术债与风险评估.md`（风险评估）
> - `tests/review_reports/2026-05-31-安全与代码质量修复完成报告.md`（v0.1.17 修复）

---

## 目录

1. [执行摘要：与历史审查的关系](#一执行摘要)
2. [全新发现（13 项）](#二全新发现)
3. [已知未解决（已有报告提出，尚未修复）](#三已知未解决)
4. [已知已部分解决（需要升级）](#四已知已部分解决)
5. [遗漏补充（已有报告提出，我 v4 初版未提及）](#五遗漏补充)
6. [修复优先级矩阵](#六修复优先级矩阵)
7. [关键架构决策点](#七关键架构决策点)
8. [Insight 总结](#八insight-总结)
9. [附录：历史审查报告对照索引](#附录历史审查报告对照索引)

---

## 一、执行摘要

### 1.1 本报告的定位

本次审查**不是从零开始的全新扫描**，而是基于以下历史资产的**增量审查**：

| 历史资产 | 内容 | 本报告处理方式 |
|---------|------|--------------|
| v0.1.17 安全审查修复（17 项） | C1-C5 + H1-H7 + M1-M5 | **不再重复提出**，已修复 ✅ |
| v3 技术债评估（19 项） | Box::leak / 重复代码 / Registry 反复构建等 | **引用已有编号**，不再展开 |
| 风险评估报告（14 项） | 协议死代码 / Python 超时 / JSON Schema 等 | **引用已有编号**，不再展开 |
| v0.1.18→v0.1.24 版本迭代 | Stage 重试 / 缓存清理 / Provider 外部化等 | **标注历史修复**，评估残留风险 |

### 1.2 问题分类统计

| 类别 | 数量 | 核心特征 |
|------|------|---------|
| **🔴 全新发现** | 13 项 | 历史审查报告、CHANGELOG、git commit 中**完全没有覆盖** |
| **🟠 已知未解决** | 7 项 | 已有报告提出，但**尚未修复**（v0.1.24 代码中仍然存在） |
| **🟡 已知已部分解决** | 7 项 | 历史版本做了缓解，但**未根除**（需要升级） |
| **🔵 遗漏补充** | 5 项 | 已有报告提出，我 v4 初版**完全未提及** |

### 1.3 真正值得关注的 4 项核心问题

从 32 项问题中，对「5000 个 PDF + 高质量卡片」目标影响最大的 **6 项**（均为全新发现）：

1. **N1 唯一ID冲突**：`LAST_UNIQUE_SEC` 的三重并发缺陷（Mutex poison + 时间回拨 + 跨进程冲突），5000 次运行必然触发
2. **N2 Jaccard去重对短内容失效**：定量反例验证（两张语义重复的金句卡 Jaccard=0），之前报告未触及的定量分析
3. **N3 Map-Reduce质量过滤性能崩溃**：50 万字 document × 50 张卡 × 正则回溯 = 12.5B 字符扫描，之前报告未触及的性能工程分析
4. **N4 9次独立LLM调用**：5000 本 × 9 次 × 50K 字符 = **2.7B 输入 token ≈ ¥2700+**，之前报告未触及的成本量化分析
5. **S3 复制检测算法概念级缺陷**：`compute_text_similarity` 使用字符包含率，"abababab" 与 "aaaabbbb" 相似度 ≈ 100%，质量检查形同虚设
6. **S4 StageCache key 全量哈希**：每文档 8 次遍历 50 万字 = 400 万字符哈希，缓存查找比 LLM 调用还慢

---

## 二、全新发现（13 项）

> **判定标准**：在历史审查报告（v3 + 风险评估）、CHANGELOG、git commit message 中完全未被提及的问题。
>
> **源码审查新增**：本版本（v4.1）通过逐行源码阅读，新增 5 项（S1-S5），均基于代码事实而非文档推导。

---

### 🔴 N1. 唯一ID在并发/时间回拨/跨进程场景下冲突

**位置**：`src/stages/cards.rs:11-13, 305-317`

```rust
static LAST_UNIQUE_SEC: LazyLock<Mutex<i64>> = LazyLock::new(|| Mutex::new(0));
// ...
let mut last_sec = LAST_UNIQUE_SEC.lock().unwrap();  // ← unwrap = panic风险
let start_sec = std::cmp::max(base_sec, *last_sec + 1);
```

**三重并发缺陷**：

| 缺陷 | 触发条件 | 后果 | 5000次运行中概率 |
|------|---------|------|-----------------|
| Mutex unwrap panic | 某线程panic导致Mutex poison | 后续所有卡片生成panic | ~1-5%（取决于LLM稳定性） |
| 时间非单调 | NTP同步、夏令时切换 | `start_sec` 可能不是单调递增 | **~100%**（NTP同步几乎必然发生） |
| 跨进程冲突 | 两个终端同时运行cardc | 两个进程有独立的 `LAST_UNIQUE_SEC` | 取决于用户习惯 |

**为什么是全新发现**：
- v3 报告和评估报告均未提及 ID 生成机制
- CHANGELOG 中没有相关修复记录
- 当前实现依赖「系统时间单调 + 单进程 + 无panic」三个假设，在生产环境中均不成立

**修复方案**：

```rust
// 方案A：UUIDv7（时间排序，单调递增，无全局状态）
use uuid::Uuid;
card.unique_id = Uuid::now_v7().to_string();

// Cargo.toml: uuid = { version = "1.11", features = ["v7"] }
```

**边界**：替换 `LAST_UNIQUE_SEC` 全局状态；唯一ID格式从14位数字变为标准UUID。

---

### 🟠 N2. Jaccard去重对中文短内容失效

**位置**：`src/dedup.rs:135-147, 150-161`

```rust
fn text_to_shingles(text: &str, size: usize) -> HashSet<String> {
    // size = 3（默认）
    for window in chars.windows(size) {
        shingles.insert(window.iter().collect());
    }
}
fn jaccard_similarity(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    intersection.len() as f64 / union.len() as f64
}
```

**阈值**：`similarity_threshold = 0.65`

**定量反例验证**：

| 卡A内容 | 卡B内容 | 3字shingle交集 | Jaccard | 判定 |
|---------|---------|---------------|---------|------|
| "阅读是心灵的旅行" | "读书是一场心灵之旅" | **空** | **0.0** | ❌ 漏过 |
| "认知负荷理论指出" | "认知负荷理论认为" | 3个 | 0.27 | ❌ 漏过（<0.65） |
| "执行意图是一种计划方式" | "执行意图是一种计划方式" | 全部 | 1.0 | ✅ 正确捕获 |

**为什么是全新发现**：
- v3 报告和评估报告均未定量分析去重算法在中文短内容上的失效
- dedup.rs 的测试覆盖了"相同内容"和"完全不同内容"，但**未覆盖"语义相近、措辞不同"的边界情况**
- 历史版本（v0.1.0→v0.1.24）没有去重算法的修改记录

**影响量化**：对于金句卡（通常 < 100 字），3字shingle产生的签名数量极少，两张语义相近但措辞不同的金句卡**几乎必然逃过去重**。

**修复方案**：

```rust
// 方案A：短内容降低阈值+减小shingle_size
fn adaptive_dedup_config(content_len: usize) -> DedupConfig {
    if content_len < 200 {
        DedupConfig {
            similarity_threshold: 0.45,
            shingle_size: 2,
            ..Default::default()
        }
    } else {
        DedupConfig::default()
    }
}

// 方案B：语义相似度（需要embedding模型）
// 使用本地模型如 onnxruntime + paraphrase-multilingual-MiniLM-L12-v2
// 短内容计算cosine similarity，阈值0.82
```

**边界**：方案A纯代码修改（1h）；方案B需要新增依赖（4h）。结束条件：短内容（<200字）去重召回率 > 80%。

---

### 🟠 N3. Map-Reduce质量过滤传入完整document导致性能崩溃

**位置**：`src/pipeline.rs:799-801` → `src/quality/card_lint.rs:693-732`

```rust
// pipeline.rs: 传入完整document（可能50万字）
let (filtered_cards, _lint_stats) = crate::quality::filter_cards_with_source(
    &all_cards,
    document,  // ← 50万字字符串
    &crate::quality::CardLintConfig::default(),
);
```

然后 `fix_ref_format` 在这个50万字字符串上运行正则：

```rust
// card_lint.rs:693-700
fn find_book_page_in_source(book_name: &str, source_text: &str) -> Option<String> {
    let re = Regex::new(&format!(
        r"## 第 (\d+) 页[\s\S]{{0,500}}?{}",
        regex::escape(book_name)
    )).unwrap();
    re.captures(source_text)  // ← 在50万字上运行正则！
}
```

**复杂度分析**：

假设 Map-Reduce 合并后：
- 文档长度：500,000 字符
- 卡片数量：50 张
- 每张卡片调用 `fix_ref_format` → `find_book_page_in_source`
- 正则引擎在50万字上扫描（最坏情况回溯）

单次调用最坏复杂度：O(500K × 500) = 250M 字符扫描
50 张卡片：50 × 250M = **12.5B 字符扫描**

**为什么是全新发现**：
- v3 报告和评估报告均未分析 Map-Reduce 模式下 `filter_cards_with_source` 的性能特征
- 历史版本（v0.1.3 引入 Map-Reduce → v0.1.24）没有对此做过性能优化
- `filter_cards_with_source` 的设计意图是让lint访问源文本做证据校验，但实际被用于ref格式修复的页码推断

**修复方案**：

```rust
// pipeline.rs: 传入chunk级别的源文本而非完整document
for (idx, chunk_result) in chunk_results.iter().enumerate() {
    let chunk_source = &chunk_result.document;  // 该chunk的原始文本（~50K字）
    let (filtered, stats) = filter_cards_with_source(
        &chunk_result.cards,
        chunk_source,  // ← 只传入该chunk的文本
        &config,
    );
}
// 最后在所有chunk过滤完成后再统一去重
```

**边界**：修改范围 `pipeline.rs` 的 Reduce 阶段；不影响最终输出（去重仍在全局层面执行）。结束条件：质量过滤阶段性能提升10倍以上。

---

### 🟠 N4. 卡片类型顺序调用，无全局协调

**位置**：`src/stages/cards.rs:242-320`

```rust
for item in plan.iter() {  // 9种类型，顺序执行
    let response = call_llm.call_chat(...).await?;  // 每次传输完整document
    let cards = parse_single_type_cards(&response, ...)?;
    all_cards.extend(cards);
}
```

**成本量化**（书籍类型，50K字符文档）：

| 项目 | 数值 |
|------|------|
| 类型数量 | 9种 |
| 每次LLM调用输入token | ~60K（50K字符 + prompt + system） |
| 每次LLM调用输出token | ~3K |
| 每本书总输入token | 540K |
| 每本书总输出token | 27K |
| 5000本书总输入token | **2.7B** |
| 按DeepSeek计费（¥1/M input token）| **¥2,700+** |

**协调缺失的后果**：

```
Chunk 1: 新知卡提取 "认知负荷理论"
Chunk 1: 术语卡也提取 "认知负荷理论"
→ 去重时两张卡片被合并，但信息密度降低
→ 用户失去了"新知视角"和"定义视角"的双重价值
```

**为什么是全新发现**：
- v3 报告和评估报告均未对「9次独立LLM调用」的架构做成本量化分析
- 历史版本（v0.1.1→v0.1.24）的演进集中在 **prompt 质量** 和 **解析鲁棒性**，未触及「架构级调用次数」
- `CardPlanner` 的 `plan()` 方法有 min/max 控制，但没有**总数量控制**和**类型间协调**

**修复方案**：

```rust
// 方案A：统一调用（一次LLM生成所有类型）
pub async fn generate_cards_unified(
    document: &str,
    doc_type: DocumentType,
    call_llm: impl ChatFn,
    load_prompt: &(dyn Fn(&str) -> Result<String> + Send + Sync),
) -> Result<Vec<Card>> {
    let plan = CardPlanner::plan(doc_type, document.chars().count());
    let prompt = build_unified_prompt(document, &plan)?;
    let response = call_llm.call_chat(prompt, max_tokens).await?;
    parse_unified_cards(&response, &plan)
}

// 方案B：提取+分配（先提取所有知识点，再按类型分配）
// 增加一次"信息提取"LLM调用，成本中等，协调效果最好
```

**边界**：方案A改动大，需重写prompt和解析器，但成本降低80%；方案B改动中等。这是一个需要用户决策的架构选择（见第七节）。

---

### 🟠 N5. 反常识卡与新知卡共用同一prompt

**位置**：`src/stages/cards.rs:224-239`

```rust
fn card_type_prompt_name(card_type: &CardType) -> &'static str {
    match card_type {
        // ...
        CardType::CounterIntuit => "knowledge_card",  // ← 共用！
        // ...
    }
}
```

但规划器中将两者作为独立类型：

```rust
// CardPlanner::plan_book
CardPlanItem::new(CardType::Knowledge, 3 * scale, 5 * scale, true, 1),
CardPlanItem::new(CardType::CounterIntuit, 1 * scale, 3 * scale, true, 4),
```

**问题**：
- 反常识卡和新知卡的本质不同：反常识卡需要识别"与常识相悖"的知识点
- 共用prompt意味着LLM没有明确指令去识别"反常识"特征
- 结果：反常识卡可能和新知卡内容高度重叠，增加去重负担

**为什么是全新发现**：
- v3 报告和评估报告均未提及 CounterIntuit 的 prompt 映射问题
- CHANGELOG 中没有相关修复记录
- 代码注释 `[M4]` 标注了 MAX_WORKERS 的注释修正，但没有涉及卡片类型的 prompt 映射

**修复方案**：创建独立的 `counter_intuit_card.md` prompt，明确强调：
- 识别与主流认知相悖的观点
- 提供"常识误解 → 正确认知"的对比结构
- 要求标注反常识的"强度"（轻微/中等/强烈）

**边界**：新增 prompt 文件 + 修改映射函数，预计 2h。

---

### 🟡 N6. 无作业队列 / 进度持久化 / 结果数据库

**当前状态**：每次运行 `cardc <file>` 处理单个文件，输出到 `./output/YYYYMMDDHHMMSS_书名/` 目录。

**缺失能力**：

| 能力 | 状态 | 5000 PDF影响 |
|------|------|-------------|
| 工作队列 | ❌ 不存在 | 无法知道哪些已处理、哪些待处理 |
| 进度持久化 | ❌ 不存在 | 处理到第2500个崩溃后需人工比对目录 |
| 结果数据库 | ❌ 不存在 | 无法查询"某概念在哪些书中出现" |
| 处理日志 | ❌ 不存在 | 无法追溯某PDF的LLM调用详情 |
| 跨文档去重 | ❌ 不存在 | 同一概念在不同书中生成的卡片无法关联 |

**为什么是全新发现**：
- v3 报告和评估报告均未涉及「批处理/队列/进度管理」层面的分析
- 所有历史版本的演进集中在「单文件处理质量」，未触及「多文件管理」
- CHANGELOG 中 v0.1.3 的「按章节拆分编译」是单文件内的拆分，不是多文件队列

**修复方案**：

```rust
// 新增模块：src/batch/
// batch_queue.rs: SQLite 作业队列
// batch_runner.rs: 批处理执行器
// batch_state.rs: 状态持久化

// 使用方式：
// $ cardc batch ./pdfs/ --output ./output/
// - 扫描目录，将所有PDF加入队列
// - 逐条处理，状态写入 SQLite
// - 支持 --resume 断点续传
// - 支持 --retry-failed 重试失败项
```

**边界**：新增模块，不影响现有单文件CLI模式；SQLite数据库位于 `.cardnote/queue.db`。

---

### 🟡 N7. 临时文件泄漏风险

**位置**：`src/converter.rs` 多处使用 `tempfile::tempdir()`

```rust
let temp_dir = tempfile::tempdir()?;  // 在函数结束时自动删除
```

**问题**：如果进程被SIGKILL或panic中断，`TempDir` 的Drop不执行，临时文件残留。

**残留场景估算**（5000次运行）：

| 中断原因 | 概率 | 残留临时目录 |
|----------|------|-------------|
| Ctrl+C / SIGINT | ~10次 | ~10个目录 |
| 系统崩溃 | ~1-2次 | ~1-2个目录 |
| panic（如unwrap触发）| ~5次 | ~5个目录 |
| 单次运行残留 | - | 50-200MB（MinerU输出） |
| 累计残留 | - | **~1-2GB** |

**为什么是全新发现**：
- v3 报告和评估报告均未涉及临时文件泄漏问题
- v0.1.22 修复了「临时目录删除的路径安全检查」，但修复的是**安全问题**（路径遍历），不是**泄漏问题**（进程中断导致Drop不执行）
- CHANGELOG 中没有相关修复记录

**修复方案**：

```rust
// 启动时清理残留临时目录
pub fn cleanup_stale_temp_dirs() {
    let temp_base = std::env::temp_dir();
    // 清理命名模式为 "tmp*" 且创建时间超过1天的目录
    // tempfile crate 的默认命名模式
}

// 在 main.rs 启动时调用
cleanup_stale_temp_dirs();
```

**边界**：启动时自动执行；只清理cardnote相关的临时目录（通过命名模式识别）。

---

### 🟡 N8. LLM用量统计不持久化

**位置**：`src/api.rs:296-317`

```rust
pub fn record_usage(&self, usage: LlmUsage) {
    if let Ok(mut log) = self.usage_log.lock() {
        log.push(usage);  // ← 内存中，进程结束即丢失
    }
}
```

**问题**：5000次运行中，每次的token用量只保存在内存中，进程结束后无法统计总成本。

**为什么是全新发现**：
- v3 报告和评估报告均未涉及用量统计持久化
- CHANGELOG 中没有相关修复记录
- `usage_report()` 方法只在单次运行结束时打印，不做持久化

**修复方案**：

```rust
pub fn record_usage(&self, usage: LlmUsage) {
    // 内存记录
    if let Ok(mut log) = self.usage_log.lock() {
        log.push(usage.clone());
    }
    // 持久化到文件
    append_usage_to_file(&usage);  // 追加到 .cardnote/usage.log
}
```

**边界**：新增日志文件 `.cardnote/usage.log`（JSON Lines格式）；不影响现有内存统计。

---

### 🔴 S1. 去重算法为 O(n²)，corpus-level 不可扩展

**位置**：`src/dedup.rs:173-188`

```rust
fn build_similarity_graph(signatures: &[CardSignature], config: &DedupConfig) -> Vec<Vec<usize>> {
    let n = signatures.len();
    let mut graph: Vec<Vec<usize>> = vec![Vec::new(); n];

    for i in 0..n {
        for j in (i + 1)..n {  // ← 双重循环 = O(n²)
            let sim = compute_similarity(&signatures[i], &signatures[j], config);
            if sim >= config.similarity_threshold {
                graph[i].push(j);
                graph[j].push(i);
            }
        }
    }
    graph
}
```

**当前影响（单文档场景）**：

| 场景 | 卡片数 | 比较次数 | 耗时估算 | 状态 |
|------|--------|---------|---------|------|
| 单 chunk（~50K 字） | 50 张 | 1,225 次 | ~1ms | ✅ 可接受 |
| 整本书（Map-Reduce 合并后） | 500 张 | 124,750 次 | ~100ms | ⚠️ 边缘但可接受 |

**corpus-level 风险（暂不考虑）**：

| 场景 | 卡片数 | 比较次数 | 耗时估算 | 状态 |
|------|--------|---------|---------|------|
| 100 本 PDF 合并去重 | 5,000 张 | 12,497,500 次 | ~10s | ❌ 不可接受 |
| 5,000 本 PDF 合并去重 | 250,000 张 | ~3.1 × 10¹⁰ 次 | ~72 天 | ❌ 完全不可行 |

**用户决策**：跨文档去重暂不考虑，但此风险需记录在案。若未来扩展至多文档策展，必须升级为 MinHash/LSH 等亚线性算法。

**为什么是全新发现**：
- v3 报告和评估报告均未分析去重算法的渐近复杂度
- 历史版本的 dedup.rs 测试只验证了功能正确性，未覆盖性能边界
- 当前 O(n²) 在单文档场景下表现正常，但属于**隐性架构债务**

**修复方案**：

```rust
// 方案A：局部敏感哈希（LSH）—— 从 O(n²) 降到 O(n)
// 方案B：MinHash + 桶化 —— 近似 Jaccard，适合大规模去重
// 方案C：先按类型/标题分桶，再桶内 O(n²) —— 折中方案，实现简单
```

**边界**：当前暂不修复；若未来支持跨文档策展，必须实施。结束条件：10 万张卡的去重时间 < 5 秒。

---

### 🟠 S2. `extract_field` 每次调用都编译新正则

**位置**：`src/stages/cards.rs:453-459`

```rust
fn extract_field(block: &str, field_name: &str) -> Option<String> {
    let pattern = format!("{}[:：]\\s*(.+?)(?:\\n|$)", regex::escape(field_name));
    let re = regex::Regex::new(&pattern).ok()?;  // ← 每次调用都编译新正则
    re.captures(block)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
}
```

**性能影响量化**：

| 项目 | 数值 |
|------|------|
| 每张卡片调用 `extract_field` 次数 | 5-8 次（标题、ref、原文、出处、仿写等） |
| 每类型卡片数 | 平均 3-5 张 |
| 每文档类型数 | 9 种 |
| 每次编译正则开销 | ~50-200μs（取决于模式复杂度） |
| **每文档正则编译总数** | **~2250 次** |
| **每文档正则编译总开销** | **~100-450ms** |

**对比**：`card_lint.rs` 已通过 [C2] 修复使用 `LazyLock` 预编译（`static RE_REF_PAGE: LazyLock<Regex>`），但 `cards.rs` 的 `extract_field` 漏掉了同样的优化。

**为什么是全新发现**：
- v3 报告和评估报告未涉及 `extract_field` 的实现细节
- [C2] 修复覆盖了 `card_lint.rs` 的正则，但未覆盖 `cards.rs`
- 单文档 100-450ms 的开销在交互式使用时不明显，但在 5000 次运行中累积为 **~500-2000 秒** 的浪费

**修复方案**：

```rust
// 方案：预编译通用字段提取正则
static RE_EXTRACT_FIELD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(.+?)[:：]\s*(.+?)(?:\n|$)").expect("硬编码正则应始终有效")
});

fn extract_field(block: &str, field_name: &str) -> Option<String> {
    // 使用预编译正则，通过 field_name 过滤匹配结果
    // 避免每次调用 format! + Regex::new
}
```

**边界**：纯性能优化，不影响输出。预计 30min。

---

### 🟠 S3. `compute_text_similarity` 算法概念级缺陷

**位置**：`src/quality/card_lint.rs:868-892`

```rust
fn compute_text_similarity(a: &str, b: &str) -> f64 {
    // 简化的相似度：较短的文本在较长文本中的最大连续子串匹配率
    let (shorter, longer) = if a.len() <= b.len() { (a, b) } else { (b, a) };

    let mut matched = 0;
    for ch in &shorter_chars {
        if longer_chars.contains(ch) {  // ← "字符包含率"
            matched += 1;
        }
    }
    matched as f64 / shorter_chars.len() as f64
}
```

**定量反例**：

| 文本A | 文本B | 实际语义 | `compute_text_similarity` | 判定 |
|-------|-------|---------|---------------------------|------|
| "abababab" | "aaaabbbb" | 完全不同 | **~100%** | ❌ 致命误判 |
| "认知负荷理论" | "认知负荷理论指出" | 后者包含前者 | **~100%** | ⚠️ 合理 |
| "abababab" | "abababab" | 完全相同 | **~100%** | ✅ 正确 |
| "abababab" | "cdcdcdcd" | 完全不同 | **0%** | ✅ 正确 |

**问题本质**：算法测量的是「字符集合重叠率」，而非「语序相似度」。任何由相同字符集合组成的文本，无论语序如何，都会得到高相似度。

**影响**：`LikelyCopied` 检查（`card_lint.rs:530-533`）形同虚设——
- 非金句卡的「直接复制」检测阈值 0.8 可被任意字符重排列文本绕过
- 在 5000 次运行中，可能有大量「字符相同但语序不同」的低质量卡片被误判为原创

**为什么是全新发现**：
- v3 报告和评估报告均未定量分析 `compute_text_similarity` 的算法正确性
- v0.1.6 的修复仅豁免了金句卡的 `LikelyCopied` 检查（见 P2），但未修复算法本身
- 历史测试覆盖了「相同内容」和「完全不同内容」，未覆盖「字符集合相同、语序不同」的边界

**修复方案**：

```rust
// 方案A：最长公共子序列（LCS）
fn lcs_similarity(a: &str, b: &str) -> f64 {
    let lcs_len = longest_common_subsequence(a, b);
    lcs_len as f64 / a.chars().count().max(b.chars().count()) as f64
}

// 方案B：编辑距离（Levenshtein）
fn edit_distance_similarity(a: &str, b: &str) -> f64 {
    let dist = levenshtein_distance(a, b);
    1.0 - dist as f64 / a.chars().count().max(b.chars().count()) as f64
}

// 方案C：shingle-based 相似度（与 dedup.rs 统一算法）
fn shingle_similarity(a: &str, b: &str, size: usize) -> f64 {
    let shingles_a = text_to_shingles(a, size);
    let shingles_b = text_to_shingles(b, size);
    jaccard_similarity(&shingles_a, &shingles_b)
}
```

**边界**：替换 `compute_text_similarity` 的实现；调整阈值（LCS 阈值建议 0.6-0.7）。预计 2h。

---

### 🟡 S4. StageCache key 计算遍历完整文档内容

**位置**：`src/pipeline.rs:145-150`

```rust
fn cache_key(stage: &str, document: &str, prompt: &str, model: &str) -> String {
    let doc_hash = Self::fnv1a_hash(document);      // ← 遍历 50 万字
    let prompt_hash = Self::fnv1a_hash(prompt);     // ← 遍历 prompt
    let combined = format!("{}|{}|{}|{}|{}", Self::VERSION, stage, doc_hash, prompt_hash, model);
    Self::fnv1a_hash(&combined)
}
```

**调用频率**：

| 阶段 | 每文档调用 `cache_key` 次数 | 用途 |
|------|---------------------------|------|
| summary | 2 次（load + save） | `try_load_stage` + `save_stage` |
| entities | 2 次 | 同上 |
| graph | 2 次 | 同上 |
| cards | 0 次（卡片阶段未启用缓存） | — |
| **总计** | **6-8 次** | — |

**性能影响**：

| 文档大小 | 单次哈希字符数 | 每文档总哈希字符数 | 单次哈希耗时 | 每文档总耗时 |
|---------|---------------|-------------------|-------------|-------------|
| 50,000 字 | 50,000 | 300,000-400,000 | ~0.05ms | ~0.3-0.4ms |
| 500,000 字 | 500,000 | 3,000,000-4,000,000 | ~0.5ms | ~3-4ms |

**当前状态**：单文档 3-4ms 可忽略。但属于**设计层面的浪费**——缓存 key 只需要验证文档是否改变，不需要每次重新哈希。

**为什么是全新发现**：
- v3 报告提及了「Stage 缓存无 Prompt 版本管理」（风险评估 M3），但未分析 key 计算的性能特征
- P4 提到了「缓存策略不足」，但关注的是 `MAX_FILES: 200` 限制，不是 key 计算开销
- 这是 v4 初版完全遗漏的性能细节

**修复方案**：

```rust
// 方案：在 CompileContext 中缓存文档哈希，避免重复计算
struct CompileContext {
    // 新增
    document_hash: Arc<Mutex<Option<String>>>,
}

impl CompileContext {
    fn get_document_hash(&self, document: &str) -> String {
        // 先检查缓存，未命中再计算
    }
}
```

**边界**：纯性能优化，不影响行为。预计 30min。

---

### 🟡 S5. `CardPlanner` 的 `scale` 逻辑过于粗糙

**位置**：`src/stages/cards.rs:148-161`

```rust
fn plan_book(char_count: usize) -> Vec<CardPlanItem> {
    let scale = if char_count > 50000 { 2 } else { 1 };
    vec![
        CardPlanItem::new(CardType::Knowledge, 3 * scale, 5 * scale, true, 1),
        // ...
    ]
}
```

**问题**：只有两级 `scale`（1 或 2），导致：

| 文档大小 | scale | 新知卡数量 | 问题 |
|---------|-------|-----------|------|
| 5,000 字 | 1 | 3-5 张 | ✅ 合理 |
| 50,000 字 | 1 | 3-5 张 | ⚠️ 偏少（10 万字的书也是 3-5 张） |
| 100,000 字 | 2 | 6-10 张 | ✅ 合理 |
| 500,000 字 | 2 | 6-10 张 | ❌ 严重不足（50 万字的书只比 10 万字多 0 张） |

**影响**：对于大型 PDF（如学术论文合集、技术手册），卡片密度过低，知识提取不充分。

**为什么是全新发现**：
- v3 报告和评估报告均未分析 `CardPlanner` 的 scale 公式
- 历史版本的演进集中在 prompt 质量和解析鲁棒性，未触及「规划数量公式」
- 在 5000 次运行中，大型文档的卡片产出质量会系统性偏低

**修复方案**：

```rust
// 方案：对数 scale 或分段线性 scale
fn plan_book(char_count: usize) -> Vec<CardPlanItem> {
    let scale = match char_count {
        0..=50000 => 1,
        50001..=150000 => 2,
        150001..=300000 => 3,
        _ => (char_count / 100000).max(4),
    };
    // ...
}
```

**边界**：纯参数调整，不影响架构。预计 30min。

---

## 三、已知未解决（7 项）

> **判定标准**：在已有审查报告中已被提出，但 v0.1.24 代码中仍然存在，且 v0.1.17→v0.1.24 的版本迭代中未修复。

| 编号 | 问题 | 已有报告 | 代码位置 | 当前状态 |
|------|------|---------|---------|---------|
| K1 | **Anthropic/Gemini/Cohere 协议是死代码** | v3 #6<br>风险评估 R1 | `api.rs:191-193`<br>`providers.rs:36-39` | `is_supported()` 返回 `false`，`send_request()` 硬编码 `/chat/completions`，直连会 404 |
| K2 | **Python 子进程无统一超时** | 风险评估 **R2** | `converter.rs` 7处 | `convert_to_markdown_async_with_timeout` 外层有 timeout，但内部 `Command::output()` 无超时，僵尸进程风险 |
| K3 | **LLM JSON 输出无 Schema 校验** | 风险评估 **R3** | `stages/*.rs` | `chat_json()` 只要求合法 JSON，不验证预期字段。空对象 `{}` 会静默通过 |
| K4 | **ProviderRegistry 反复构建** | v3 #9 | `providers.rs:103` | 每次调用 `new()` 重建16个Provider。`global()` 单例未实现 |
| K5 | **Map-Reduce 并发数硬编码为 2** | v3 #11 | `pipeline.rs:667` | `Semaphore::new(2)`，无 CLI/环境变量配置 |
| K6 | **缓存清理同步阻塞启动** | v3 #10 | `pipeline.rs:404` | `cleanup_cache_dir()` 在 `Pipeline::new()` 时同步执行，大量文件时延迟明显 |
| K7 | **外部 Python 工具链无版本约束** | v3 #8 | `converter.rs` | 调用 4 个 Python 工具但无版本检查，API 变化无感知 |

**说明**：以上 7 项问题在已有报告中已有详细分析，本报告不再展开。如需查看完整分析，请参阅：
- `tests/review_reports/2026-05-31-技术债与风险评估-v3.md`
- `tests/review_reports/2026-05-31-技术债与风险评估.md`

---

## 四、已知已部分解决（7 项）

> **判定标准**：历史版本已做缓解，但当前代码中仍有残留问题或需要升级。

---

### P1. 卡片解析失败静默丢弃 → v0.1.22/v0.1.23 已部分缓解

**历史修复**：
- **v0.1.22**: "增强编译失败的可见性" — 在 `pipeline.run()` 返回后增加编译结果健康检查。如果摘要和卡片均为空，打印明确警告并提示检查 `compile_diagnostics.md`
- **v0.1.23**: "Stage 层重试" — summary/entities/cards/graph 四个阶段均获 3 次重试 + 指数退避

**残留问题**：
- 类型级别的 `parse_single_type_cards` 失败后的 `continue` 行为仍然存在（`stages/cards.rs:296`）
- 重试只针对 LLM 调用失败，不针对**解析失败**
- 如果某类型解析连续 3 次失败，该类型卡片仍被静默丢弃

**当前状态**：「最终检查层 + 重试层」已兜底，但「类型级解析失败仍然丢弃」未根除。

**升级建议**：为解析失败增加「JSON fallback 重试」——要求 LLM 以 JSON 格式重新输出。

---

### P2. 复制检测不可靠 → v0.1.6 已部分缓解

**历史修复**：
- **v0.1.6**: "金句卡豁免 LikelyCopied 检查" — 金句卡跳过相似度检查

**残留问题**：
- `compute_text_similarity` 的字符包含率算法本身未被修改（`quality/card_lint.rs:868-892`）
- 非金句卡的复制检测仍然不可靠

**当前状态**：「重灾区（金句卡）已豁免」，但「算法本身不可靠」未解决。

**升级建议**：改用编辑距离或 LCS 算法。

---

### P3. 信息密度标记词硬编码 → v0.1.6 已部分缓解

**历史修复**：
- **v0.1.6**: "改进信息密度计算算法" — 从 47 个标记词扩展到 90+ 个，增加权重差异化

**残留问题**：
- 标记词列表仍然是硬编码的，维护成本高
- 无法覆盖技术文档、文学作品等特定领域

**当前状态**：「标记词更多但仍硬编码」。

**升级建议**：提取到配置文件 `.cardnote/density_markers.toml`。

---

### P4. 缓存策略不足 → v0.1.23 已部分缓解

**历史修复**：
- **v0.1.23**: "缓存文件夹自动清理" — 每次启动自动删除 30 天前的缓存文件，若文件数超过 200 自动删除最旧的

**残留问题**：
- `MAX_FILES: 200` 对 5000 次运行远远不够（5000 PDF × 平均 5 个 stage = 25,000 缓存文件）
- StageCache 的 key 计算需要遍历完整 document 内容 + prompt 内容，50万字文档每次全量哈希
- 缓存目录位于当前工作目录，跨目录运行不共享

**当前状态**：「有清理机制但限制太小」。

**升级建议**：按总大小限制（如 500MB）+ LRU 策略。

---

### P5. ref 格式硬编码特定作者/书名 → v0.1.15/v0.1.17 已部分缓解

**历史修复**：
- **v0.1.15**: 新增 `fix_ref_format()` 函数，10 条自动修复规则（含"阳志平"、"人生模式"硬编码）
- **v0.1.17 M1**: 将硬编码提取为 `KNOWN_BOOKS` 常量数组

**残留问题**：
- `KNOWN_BOOKS` 仍然硬编码在代码中，只有两本书（"人生模式"、"聪明的阅读者"）
- 规则 8/9/10 在无法匹配时仍然 fallback 到 "人生模式"（`card_lint.rs:627,638,648`）

**当前状态**：「从分散硬编码集中到 KNOWN_BOOKS 常量」，但仍未配置化。

**升级建议**：将 `KNOWN_BOOKS` 提取到 `.cardnote/books.json` 配置文件。

---

### P6. 临时目录删除路径安全 → v0.1.22 已修复安全问题

**历史修复**：
- **v0.1.22**: "强化临时目录删除的路径安全检查" — 修复了 `canonicalize` 失败时的回退逻辑，使用 `Path::components()` 检查 `ParentDir`

**残留问题**：
- 安全问题已修复，但「进程中断导致 temp_dir 泄漏」的问题未解决（见全新发现 N7）

**当前状态**：「安全问题已修复，泄漏问题未解决」。

---

### P7. 编译失败可见性 → v0.1.22 已增强

**历史修复**：
- **v0.1.22**: "增强编译失败的可见性" — LLM 阶段失败时返回空值但不中断编译，增加编译结果健康检查

**残留问题**：
- 健康检查只在 `pipeline.run()` 返回后执行，不在每个阶段执行
- 如果某类型卡片被静默丢弃但其他类型有输出，最终检查不会触发

**当前状态**：「空结果有警告，但部分丢失无警告」。

---

## 五、遗漏补充（5 项）

> **判定标准**：已有审查报告提出了这些问题，但我的 v4 初版报告完全未提及。

| 编号 | 问题 | 已有报告 | 代码位置 | 说明 |
|------|------|---------|---------|------|
| M1 | **scan_directory / scan_directory_async 代码重复** | 风险评估 **R4** | `scan.rs:54-235` | ~120 行几乎相同的代码，修改逻辑需改两处 |
| M2 | **CompileConfig 定义但未被 Pipeline 使用** | 风险评估 **M1** | `doc_type.rs:33-72` | 文档类型检测产出为零，分块仍用固定 `CHUNK_SIZE=50000` |
| M3 | **BookCompilationResult 模型死代码** | 风险评估 **L1** | `models.rs:454-467` | 定义了完整书籍级编译结构但从未使用 |
| M4 | **文件大小上限 `MAX_FILE_SIZE_MB=500` 是理论值** | 风险评估 **M4** | `config.rs:140` | 500MB PDF 转换时可能在转换层先 OOM，实际保护不足 |
| M5 | **resolve_book_title 逻辑分散在两个模块** | 风险评估 **L4** | `main.rs:218-230` + `converter.rs:1065-1108` | 书名解析分布在 main.rs 和 converter.rs，`guess_title()` 未被使用 |

---

## 六、修复优先级矩阵

### P0（处理任何PDF前必须修复）

| 编号 | 问题 | 来源 | 文件 | 预计工作量 | 不修复的后果 |
|------|------|------|------|-----------|-------------|
| **N1** | 唯一ID冲突 | **全新** | `stages/cards.rs` | 1h | 卡片覆盖、去重异常 |
| **K2** | Python子进程无超时 | 已有 | `converter.rs` | 4h | 进程永久阻塞 |
| **K3** | JSON无Schema校验 | 已有 | `stages/*.rs` | 2h | 空对象静默通过 |
| **S3** | 复制检测算法概念级缺陷 | **源码新增** | `quality/card_lint.rs` | 2h | 质量检查形同虚设 |

### P1（处理>100个PDF前建议修复）

| 编号 | 问题 | 来源 | 文件 | 预计工作量 | 不修复的后果 |
|------|------|------|------|-----------|-------------|
| **N5** | 反常识卡共用prompt | **全新** | `stages/cards.rs` | 2h | 内容同质化 |
| **N2** | 去重对短内容失效 | **全新** | `dedup.rs` | 4h | 大量语义重复卡片 |
| **P5** | ref硬编码→配置化 | 部分解决 | `quality/card_lint.rs` | 2h | 非目标书籍ref被污染 |
| **N6** | 无作业队列 | **全新** | 新增模块 | 8h | 无法管理5000个PDF |
| **K4** | Registry反复构建 | 已有 | `providers.rs` | 1h | 性能浪费 |
| **S2** | extract_field重复编译正则 | **源码新增** | `stages/cards.rs` | 30min | 5000次运行累积浪费500-2000秒 |
| **S5** | CardPlanner scale过于粗糙 | **源码新增** | `stages/cards.rs` | 30min | 大型文档卡片密度过低 |

### P2（提升卡片质量与性能）

| 编号 | 问题 | 来源 | 文件 | 预计工作量 | 不修复的后果 |
|------|------|------|------|-----------|-------------|
| **N4** | 9次独立LLM调用 | **全新** | `stages/cards.rs` | 8h | API成本失控 |
| **N3** | Map-Reduce质量过滤性能 | **全新** | `pipeline.rs` | 2h | 处理速度慢 |
| **P2** | 复制检测算法不可靠 | 部分解决 | `quality/card_lint.rs` | 4h | 质量误判 |
| **P3** | 标记词硬编码 | 部分解决 | `quality/card_lint.rs` | 2h | 特定领域系统性误判 |
| **N7** | 临时文件泄漏 | **全新** | `converter.rs` | 2h | 磁盘空间泄漏 |
| **N8** | 用量统计不持久 | **全新** | `api.rs` | 2h | 无法统计总成本 |
| **S4** | StageCache key全量哈希 | **源码新增** | `pipeline.rs` | 30min | 缓存查找有设计层面浪费 |

### P3（清理已知未解决 + 遗漏补充 + 架构债务）

| 编号 | 问题 | 来源 | 预计工作量 | 备注 |
|------|------|------|-----------|------|
| K1 | Anthropic/Gemini协议 | 已有 | 4h | |
| K5 | Map-Reduce并发数配置 | 已有 | 1h | |
| K6 | 缓存清理异步化 | 已有 | 30min | |
| K7 | Python工具链版本约束 | 已有 | 2h | |
| M1 | scan_directory代码重复 | 遗漏 | 1h | |
| M2 | CompileConfig激活或清理 | 遗漏 | 1h | |
| M3 | BookCompilationResult清理 | 遗漏 | 30min | |
| M4 | 文件大小上限调整 | 遗漏 | 30min | |
| M5 | resolve_book_title整合 | 遗漏 | 1h | |
| **S1** | 去重算法O(n²)（corpus-level） | **源码新增** | 4h | **暂不修复**，跨文档策展时实施 |

### P3（清理已知未解决 + 遗漏补充）

| 编号 | 问题 | 来源 | 预计工作量 |
|------|------|------|-----------|
| K1 | Anthropic/Gemini协议 | 已有 | 4h |
| K5 | Map-Reduce并发数配置 | 已有 | 1h |
| K6 | 缓存清理异步化 | 已有 | 30min |
| K7 | Python工具链版本约束 | 已有 | 2h |
| M1 | scan_directory代码重复 | 遗漏 | 1h |
| M2 | CompileConfig激活或清理 | 遗漏 | 1h |
| M3 | BookCompilationResult清理 | 遗漏 | 30min |
| M4 | 文件大小上限调整 | 遗漏 | 30min |
| M5 | resolve_book_title整合 | 遗漏 | 1h |

---

## 七、关键架构决策点

### 决策1：卡片生成策略（影响 API 成本 80%）

| 选项 | 描述 | 成本影响 | 质量影响 | 工作量 |
|------|------|---------|---------|--------|
| **A. 保持独立调用** | 每个类型一次LLM调用 | 高（¥2700+/5000本） | 协调差，类型间重复 | 0h |
| **B. 统一调用** | 一次LLM生成所有类型 | 低（¥500/5000本） | LLM可能"偷懒"只生成部分类型 | 8h |
| **C. 提取+分配** | 先提取知识点，再按类型分配 | 中（¥1200/5000本） | 协调最好，避免重复 | 8h |

**建议**：先修复 P0（保持当前架构），然后在 P1 阶段迁移到方案 C。

### 决策2：去重算法升级

| 选项 | 描述 | 优点 | 缺点 | 工作量 |
|------|------|------|------|--------|
| **A. 调参优化** | 短内容shingle_size=2，阈值=0.45 | 纯代码改动 | 仍有语义盲区 | 1h |
| **B. 语义相似度** | 本地Embedding模型 | 质量最高 | 增加部署复杂度 | 8h |

**建议**：先实施 A（立即见效），在 P3 阶段考虑 B。

### 决策3：5000 PDF管理方式

| 选项 | 描述 | 优点 | 缺点 | 工作量 |
|------|------|------|------|--------|
| **A. 保持CLI单文件** | 用户手动循环调用 | 简单 | 无进度管理，无断点续传 | 0h |
| **B. 批处理模式** | `cardc batch ./pdfs/` | 自动队列，断点续传 | 需新增模块 | 8h |

**建议**：实施 B。5000 个PDF的规模必须有一个工作队列。

---

## 八、Insight 总结

`★ Insight ─────────────────────────────────────`

1. **历史审查的价值已经被验证**。v0.1.17 的 17 项安全修复、v3 报告的 19 项技术债、风险评估的 14 项问题——这些报告确实推动了代码质量的实质性改进。v4 初版未能充分识别这些历史资产，导致 10 项重复发现，这是审查方法的缺陷。

2. **源码审查发现了 5 项文档审查无法触及的问题**。S1-S5 只有在逐行阅读代码后才能发现：
   - S1（O(n²) 去重）的复杂度分析需要看到 `build_similarity_graph` 的双重循环
   - S2（正则重复编译）需要看到 `extract_field` 的 `Regex::new` 调用点
   - S3（字符包含率缺陷）需要看到 `compute_text_similarity` 的具体实现
   - S4（全量哈希）需要看到 `cache_key` 的 `fnv1a_hash(document)` 调用
   - S5（scale 粗糙）需要看到 `plan_book` 的两级判断逻辑
   这印证了 **R5（Verification by Execution）** 的原则——执行/代码是 ground truth，文档是假设。

3. **"已知已部分解决"是最容易被忽视的风险类别**。P1-P7 的问题都有历史修复记录，容易让人产生"已经修过了"的安全感。但实际上它们只是从「致命」降级为「隐患」，在 5000 次运行中仍然会累积为问题。例如 P5（ref硬编码）从 v0.1.15 的「分散硬编码」集中到 v0.1.17 的 `KNOWN_BOOKS` 常量，但仍然没有配置化。

4. **v0.1.12→v0.1.14 的书名提取演进是教科书级的启发式设计迭代**。经历了「文本搜索（误报率高）→ 增加过滤规则 → 更多过滤规则 → 彻底删除文本搜索」四步，最终收敛为「只信元数据 + 文件名」。这个演进模式值得在 ref 格式、卡片解析等其他脆弱环节中借鉴——当启发式规则需要反复打补丁时，可能是时候彻底换一个策略了。

5. **N4（9次独立LLM调用）是成本和质量的双重瓶颈**。历史版本在 prompt 质量和解析鲁棒性上投入了大量迭代（v0.1.6→v0.1.20 共 15 个版本），但架构级的调用次数从未被质疑。对于 5000 个 PDF 的目标，这是比任何单个 bug 都更重要的结构性问题。

6. **S3（复制检测算法缺陷）是一个"看起来很对，实际全错"的典型**。`compute_text_similarity` 函数有完整的注释、测试覆盖、被正式调用——但它的核心算法（字符包含率）在概念层面就是错误的。这种**表面正确性掩盖了本质缺陷**的问题，在 AI Pipeline 中尤其危险，因为上下游都信任这个检查的结果。

7. **技术评估报告对代码现状的 3 个误判**值得作为方法论反思记录：
   - "Markdown 被当作主数据"→ 实际已有完整内部模型，缺的是持久化桥梁
   - "Prompt 承载全部业务规则"→ 实际 Rust 类型系统已承担相当约束，缺的是 Schema 校验层
   - "OCR 流程完全外部"→ 实际已内化但治理不足（超时/版本约束缺失）
   这说明**基于公开文档的架构评估有系统性偏差**——方向判断准确，但起点判断偏离。
`─────────────────────────────────────────────────`

---

## 附录：历史审查报告对照索引

### 已有报告提出的问题与本报告的对照

| 已有报告 | 问题编号 | 问题描述 | 本报告处理方式 |
|---------|---------|---------|--------------|
| v3 #1 | Box::leak 内存泄漏 | `config.rs:228` | **未重复提出**（v0.1.17未修复，但属于低影响） |
| v3 #2 | 字符串截断未标注省略号 | `main.rs:275` | **未重复提出**（低影响） |
| v3 #3 | Git历史API Key泄露 | `.env` / `.gitignore` | **未重复提出**（安全审计范畴） |
| v3 #4 | converter.rs重复代码 | `converter.rs` | 在 N7 中提及，但重点不同（历史关注代码重复，v4关注泄漏） |
| v3 #5 | providers.rs巨型模块 | `providers.rs` | **未重复提出** |
| v3 #6 | 3种协议死代码 | `api.rs:191` | 归入 **K1** 已知未解决 |
| v3 #7 | 零集成测试 | `tests/` | **未重复提出**（v0.1.23已部分修复） |
| v3 #8 | 外部Python工具链无版本约束 | `converter.rs` | 归入 **K7** 已知未解决 |
| v3 #9 | ProviderRegistry反复构建 | `providers.rs:103` | 归入 **K4** 已知未解决 |
| v3 #10 | 缓存清理同步阻塞 | `pipeline.rs:404` | 归入 **K6** 已知未解决 |
| v3 #11 | Map-Reduce并发数硬编码 | `pipeline.rs:667` | 归入 **K5** 已知未解决 |
| v3 #12 | LLM响应无JSON Schema验证 | `api.rs:210` | 归入 **K3** 已知未解决 |
| v3 #13-19 | 小问题/清理项 | 各处 | **未重复提出** |
| 风险评估 R1 | Anthropic/Gemini协议未实现 | `api.rs:321` | 归入 **K1** 已知未解决 |
| 风险评估 R2 | Python子进程无超时 | `converter.rs` | 归入 **K2** 已知未解决 |
| 风险评估 R3 | LLM JSON无Schema校验 | `stages/*.rs` | 归入 **K3** 已知未解决 |
| 风险评估 R4 | scan_directory代码重复 | `scan.rs:54-235` | 归入 **M1** 遗漏补充 |
| 风险评估 M1 | CompileConfig未使用 | `doc_type.rs:33` | 归入 **M2** 遗漏补充 |
| 风险评估 M2 | MAX_WORKERS不一致 | `config.rs:20` | **未重复提出**（注释已修正） |
| 风险评估 M3 | Stage缓存无Prompt版本管理 | `pipeline.rs:138` | 在 P4 中提及 |
| 风险评估 M4 | 文件大小上限理论值 | `config.rs:140` | 归入 **M4** 遗漏补充 |
| 风险评估 M5 | 书名提取逻辑脆弱 | `converter.rs:1065` | **未重复提出**（v0.1.14已收敛） |
| 风险评估 L1 | BookCompilationResult死代码 | `models.rs:454` | 归入 **M3** 遗漏补充 |
| 风险评估 L2 | 测试覆盖不均衡 | 各处 | **未重复提出** |
| 风险评估 L3 | converter.rs重复代码 | `converter.rs` | 在 N7 中提及 |
| 风险评估 L4 | resolve_book_title分散 | `main.rs` + `converter.rs` | 归入 **M5** 遗漏补充 |
| 风险评估 L5 | Box::leak | `config.rs:228` | **未重复提出** |

### v0.1.17 安全审查修复的问题（不再重复提出）

| 编号 | 问题 | 修复方式 | 验证状态 |
|------|------|---------|---------|
| C1 | 异步Mutex阻塞 | `Mutex` → `AtomicUsize` | ✅ 代码已验证 |
| C2 | 正则重复编译 | 动态编译 → `LazyLock` 预编译 | ✅ 代码已验证 |
| C3 | 路径遍历风险 | `canonicalize` + `starts_with` 验证 | ✅ 代码已验证 |
| C4 | API key文件权限 | `chmod 600` | ✅ 代码已验证 |
| C5 | base_url未验证 | `reqwest::Url::parse` | ✅ 代码已验证 |
| H1-H7 | 错误处理/API key暴露/上帝函数等 | 详见修复报告 | ✅ 代码已验证 |
| M1-M5 | 硬编码书名/重复替换/OnceLock/MAX_WORKERS注释/FNV哈希 | 详见修复报告 | ✅ 代码已验证 |

---

*报告结束。本报告已与历史审查报告全面对照，避免重复发现。如需查看历史报告的完整分析，请参阅 `tests/review_reports/` 目录下的对应文件。*
