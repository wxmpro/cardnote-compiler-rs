# CardNote Compiler — 生产级验收标准（v1.0）

> **目标**：GitHub 公开项目，1000+ Star，5000 本经典书籍 PDF → 高质量知识卡片
> **原则**：每一项标准必须是**可验证、可量化、无歧义**的。不允许"基本通过""差不多"等模糊描述。
> **验收方式**：每个检查项前标注验证方法 —— `[EXEC]` 执行验证 / `[INSP]` 代码审查 / `[TEST]` 自动化测试

---

## 一、卡片内容质量（Prompt 层）— 12 项

### 1.1 ref 格式规范

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-P-001 | **ref 格式统一**：任意卡片（新知卡、术语卡、反常识卡、综述卡、行动卡、金句卡、人物卡、事件卡、图示卡、新词卡、基础卡、索引卡）的 `ref` 字段必须匹配正则 `^[^《》\s]+_p\d+(?:-\d+)?$`。不区分中英文书名。 | `[TEST]` 编写单元测试：用 100 张随机抽样卡片验证 ref 格式合规率 = 100% |
| AC-P-002 | **禁止书名号**：任何卡片的 ref 字段不得包含 `《` 或 `》` 字符 | `[TEST]` 正则 `[《》]` 对全部卡片 ref 扫描，匹配数 = 0 |
| AC-P-003 | **禁止"本书"替代**：任何卡片的 ref 字段不得以 `本书` 开头 | `[TEST]` 正则 `^本书` 对全部卡片 ref 扫描，匹配数 = 0 |
| AC-P-004 | **禁止章节名替代**：ref 中 `_p` 之前的部分必须是完整书名，不能是篇章名（如"行动模式""人际模式"） | `[TEST]` ref 来源名必须在 `.cardnote/books.json` 的 known_books 数组的 name 或 aliases 中存在；不存在则标记为 `Rejected` |
| AC-P-005 | **禁止前导零**：ref 中 `_p` 后的页码不得有前导零（`p049` 为非法，`p49` 为合法） | `[TEST]` 正则 `_p0\d+` 对全部卡片 ref 扫描，匹配数 = 0 |
| AC-P-006 | **禁止作者名前缀**：ref 不得包含作者名（如 "阳志平《人生模式》_p268" 为非法，应为 "人生模式_p268"） | `[TEST]` ref 中 `_p` 之前部分不得匹配已知作者名列表 |
| AC-P-007 | **引用其他书籍时格式正确**：如果卡片内容引用了书中推荐的其他书，ref 仍指向当前文档，格式为 `书名_p页码（引用自《其他书名》）` | `[INSP]` 抽样 50 张含跨书引用的卡片，全部符合此格式 |
| AC-P-008 | **综述卡 ref 100% 指向当前文档**：综述卡的 ref 不得引用当前正在阅读的文档以外的其他书籍 | `[TEST]` 对全部综述卡 ref 做来源名校验，100% 通过 |

### 1.2 例子/案例真实性

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-P-009 | **禁止虚构叙事开头**：任何卡片的"例子"字段不得包含以下 LLM 虚构标记：`想象一下`、`比如有一个人`、`假设你`、`走在街上`、`小王在` | `[TEST]` 正则扫描全部卡片中以上 5 种模式的匹配数 = 0 |
| AC-P-010 | **例子可追溯至原文**：每张卡片的"例子"内容必须能在 PDF 原文中找到对应的具体段落、研究引用或真实案例。抽样 50 张卡片做对抗性交叉验证，可追溯率 ≥ 90% | `[EXEC]` 人工 + 自动化交叉验证（Jaro-Winkler 匹配原文片段） |
| AC-P-011 | **允许无例子但禁止假例子**：如果 PDF 原文无具体案例，允许卡片省略"例子"字段或标注"原文未提供具体案例"。系统不会因为缺少例子而降级卡片。但如果写了例子却无法追溯，标记 `status: NeedsRetry, reject_reason: "例子无法追溯至原文"` | `[TEST]` 所有带"例子"字段的卡片，其例子内容必须与原文有 Jaro-Winkler ≥ 0.6 的匹配片段 |

### 1.3 卡片类型差异化

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-P-012 | **反常识卡独立 prompt**：`CardType::CounterIntuit` 映射到独立 prompt 文件 `counter_intuit_card.md`，不再共用 `knowledge_card` | `[INSP]` 代码审查 `card_type_prompt_name` 函数 |
| AC-P-013 | **反常识卡与新知卡内容差异化 > 50%**：对于同一本书，反常识卡和新知卡的核心论点重叠率（关键词 Jaccard）< 50% | `[TEST]` 对 10 本书的产出计算两类卡片的平均 Jaccard 相似度 |
| AC-P-014 | **反常识卡包含"强度"字段**：每张反常识卡必须标注反常识强度（`轻微` / `中等` / `强烈`） | `[TEST]` 所有 `CardType::CounterIntuit` 卡片的 content 或独立字段包含以上三词之一 |

### 1.4 字数与结构约束

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-P-015 | **术语卡解释 100-200 字**：每张术语卡中"解释"部分（匹配 `解释[:：]` 后的内容）的字符数必须在 [100, 200] 范围内 | `[TEST]` 全部术语卡的解释字数检查，合规率 = 100% |
| AC-P-016 | **术语解释只讲核心机制**：术语卡的解释部分不得包含"历史背景""应用场景""细分子概念"等小标题或内容结构。检测到此类扩展内容时标记 `reject_reason: "术语解释过度膨胀"` | `[TEST]` 正则检测解释中是否包含"背景""应用场景""子概念"等扩展标记 |
| AC-P-017 | **专业术语自带解释**：如果卡片正文中出现专业术语（如 STC 算子、差序格局、邓巴数字），必须在同一张卡片内附带 1-2 句话（20 字以内）的解释，用括号呈现 | `[TEST]` 抽取 30 张含专业术语的卡片，检查每个术语首次出现时是否紧跟括号解释 |
| AC-P-018 | **综述卡同一篇章 ≤ 1 张**：同一篇章（相同的一级标题范围）的综述卡数量 ≤ 1。>50 页的篇章最多分为上/中/下三张且必须在标题中明确标注 `（上）`/`（中）`/`（下）` | `[TEST]` 对每本书的综述卡按标题分组，同篇章计数 ≤ 1（或标注上/中/下） |
| AC-P-019 | **一主题一卡**：核心论点重叠 > 70%（标题关键词 Jaccard + 内容 LCS 综合）的卡片在去重阶段合并为一张 | `[TEST]` 去重后输出中，任意两张卡片的核心论点相似度 < 0.70 |

---

## 二、校验算法正确性（校验层）— 8 项

### 2.1 复制检测

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-V-001 | **LCS 相似度替代字符包含率**：`compute_text_similarity` 使用最长公共子序列（LCS）算法，不再使用字符包含率 | `[INSP]` 代码审查函数实现 |
| AC-V-002 | **反例 1 通过**：`lcs_similarity("abababab", "aaaabbbb")` 返回值 < 0.30 | `[TEST]` 单元测试 |
| AC-V-003 | **反例 2 通过**：`lcs_similarity("认知负荷理论", "认知负荷理论指出")` 返回值 > 0.80 | `[TEST]` 单元测试 |
| AC-V-004 | **边界 1 通过**：`lcs_similarity("相同内容", "相同内容")` 返回值 = 1.00 | `[TEST]` 单元测试 |
| AC-V-005 | **边界 2 通过**：`lcs_similarity("完全不同的文本A", "完全不同的文本B")` 返回值 < 0.20 | `[TEST]` 单元测试 |
| AC-V-006 | **LCS 执行性能**：对 500 字符 × 500 字符的文本对，LCS 计算耗时 < 10ms | `[TEST]` 性能基准测试 |

### 2.2 去重算法

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-V-007 | **自适应 Jaccard 实现**：`DedupConfig` 根据内容长度自适应选择 shingle_size 和阈值：<100 字 → 2-shingle + 阈值 0.40；100-200 字 → 2-shingle + 阈值 0.45；>200 字 → 3-shingle + 阈值 0.65 | `[INSP]` 代码审查 `adaptive_dedup_config` 函数 |
| AC-V-008 | **短内容去重反例 1**：`jaccard_similarity("阅读是心灵的旅行", "读书是一场心灵之旅")`（使用自适应配置）> 0.40 | `[TEST]` 单元测试 |
| AC-V-009 | **短内容去重反例 2**：`jaccard_similarity("认知负荷理论指出", "认知负荷理论认为")`（使用自适应配置）> 0.40 | `[TEST]` 单元测试 |
| AC-V-010 | **相同内容**：`jaccard_similarity("执行意图是一种计划方式", "执行意图是一种计划方式")` = 1.00 | `[TEST]` 单元测试 |
| AC-V-011 | **短内容去重召回率**：对 100 组人工标注的"语义重复"短内容对（<200 字），自适应 Jaccard 召回率（相似度 ≥ 阈值的比例）> 80% | `[TEST]` 标准测试集验证 |

### 2.3 质量评分

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-V-012 | **评分区分度**：2 个 Critical issue 的卡片得分 ≠ 10 个 Minor issue 的卡片得分。2 Critical → 0.10 ≤ score ≤ 0.30；10 Minor → 0.40 ≤ score ≤ 0.60 | `[TEST]` 单元测试构造两种 issue 组合验证 |
| AC-V-013 | **评分单调性**：增加任意 issue 后，评分不得上升（严格非增） | `[TEST]` 单元测试 |
| AC-V-014 | **评分边界**：无 issue → 1.00；任意数量 issue → 最低 0.10（保留区分度，不到 0） | `[TEST]` 单元测试 |
| AC-V-015 | **分段线性评分实现**：Critical 每项扣 0.4（最低 0.1），Major 每项扣 0.15（最低 0.5），Minor 每项扣 0.05（最低 0.7） | `[INSP]` 代码审查 `compute_card_quality_score` 函数 |

### 2.4 解析容错

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-V-016 | **JSON fallback 重试**：标准解析失败后，自动触发一次 JSON 格式重试（要求 LLM 以纯 JSON 格式重新输出）。重试仍失败则记录 `compile_diagnostics.md`，标记该类型为 `ParseFailed` | `[EXEC]` 用格式错误的 mock 响应验证 fallback 链路 |
| AC-V-017 | **不静默丢弃**：某卡片类型连续 3 次解析失败（含 1 次 fallback），不得导致该类型卡片完全丢失。最多丢失当前批次的该类型卡片 | `[TEST]` 用 3 次失败 mock 验证不丢其他类型卡片 |

---

## 三、质量门控（输出层）— 5 项

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-QG-001 | **reject_reason 拦截**：任何 `reject_reason` 字段非空的卡片**不得**写入最终 Markdown 输出文件（`all_cards.md` 和各类型的 `.md` 文件） | `[TEST]` 构造 10 张带 reject_reason 的卡片，执行 `save_single`，验证最终输出中 0 张卡片含 reject_reason |
| AC-QG-002 | **status 拦截**：`status != Accepted` 的卡片（NeedsRetry / Degraded / Rejected）不得进入最终 Markdown 输出。仅写入 `card_quality_report.md` | `[TEST]` 同上验证 |
| AC-QG-003 | **质量报告完整性**：所有被拦截的卡片（含 reject_reason、status、quality_score）必须在 `card_quality_report.md` 中完整列出，不得遗漏 | `[TEST]` 验证被拦截卡片数量 = 质量报告中"需关注卡片"数量 |
| AC-QG-004 | **类型混淆检测**：术语卡的标题不得是卡片类型名称（"反常识卡""新知卡""术语卡""金句卡""综述卡""行动卡"等）。检测到时标记 `status: Rejected, reject_reason: "类型混淆：术语卡标题为卡片类型名称"` | `[TEST]` 单元测试 |
| AC-QG-005 | **JSON 空对象拦截**：`chat_json()` 返回的 JSON 至少包含 `title` 字段；若无此字段，触发 JSON fallback 重试，而非标记为 Accepted | `[TEST]` Mock `{"unrelated": "data"}` 返回值验证触发重试 |

---

## 四、数据完整性与安全 — 5 项

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-DI-001 | **UUIDv7 唯一 ID**：卡片 `unique_id` 使用 `uuid::Uuid::now_v7()` 生成，不再使用基于时间戳 + Mutex 的方案 | `[INSP]` 代码审查 |
| AC-DI-002 | **并发零碰撞**：并发生成 10,000 个 UUIDv7，全部唯一 | `[TEST]` 并发测试 |
| AC-DI-003 | **时间回拨不冲突**：系统时间回拨（NTP 同步）后生成的 UUIDv7 不与之前生成的 ID 重复 | `[TEST]` 模拟时间回拨场景 |
| AC-DI-004 | **跨进程不冲突**：两个独立进程同时生成 UUIDv7，所有 ID 唯一 | `[TEST]` 多进程测试 |
| AC-DI-005 | **Prompt 注入净化**：PDF 内容直接插入 prompt 前，必须经过净化处理。包含 `ignore previous instructions`、`forget all rules`、`忘记之前的指令`、`忽略以上` 等注入模式的文本被替换为等长屏蔽字符 `█` | `[TEST]` 构造含注入模式的 PDF 文本，验证净化后不含原始注入模式 |

---

## 五、可靠性（超时与容错）— 6 项

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-RL-001 | **Python 子进程超时**：任意 `Command::output()` 调用有步骤级超时（5 分钟）。超时后强制终止子进程，返回明确超时错误，不挂死 | `[EXEC]` 用 `sleep 600` mock Python 脚本验证 5 分钟后被终止 |
| AC-RL-002 | **无僵尸进程**：连续处理 100 个 PDF 后，系统中 cardnote 相关的 Python 僵尸进程数 = 0 | `[EXEC]` `ps aux | grep python | grep -c defunct` 验证 |
| AC-RL-003 | **外层超时兜底**：`convert_to_markdown_async_with_timeout` 的外层 `tokio::time::timeout` 正确工作。超时后返回 `AppError::Timeout` 而非 panic | `[TEST]` Mock 超时场景验证返回错误类型 |
| AC-RL-004 | **JSON 解析降级路径**：LLM 返回非法 JSON → 触发一次重试（要求 JSON 格式）→ 仍失败 → 尝试文本解析提取卡片 → 仍失败 → 返回 `Err` 并记录诊断信息 | `[TEST]` Mock 非法 JSON 响应验证完整降级链路 |
| AC-RL-005 | **Prompt 文件缺失 Fallback**：某卡片类型的 prompt 文件缺失时，自动 fallback 到 `all_cards.md` 通用格式，生成的卡片标记 `status: Degraded, degraded_from: <原CardType>`，不导致该类型卡片完全丢失 | `[TEST]` 删除一个 prompt 文件后运行编译验证 |
| AC-RL-006 | **API 限流自动退避**：当请求速率接近配置的 RPM 上限时，自动等待下一个可用窗口，不触发 429 错误 | `[EXEC]` 高速连续发送 20 个请求验证无 429 返回 |

---

## 六、性能与成本 — 6 项

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-PF-001 | **LLM 调用次数**：单本书的 LLM 调用次数 ≤ 3 次（提取 1 次 + 分配生成 1-2 次）。不采用旧的 9 次独立调用架构 | `[EXEC]` 运行 10 本书，统计每本平均调用次数 |
| AC-PF-002 | **旧策略 fallback**：新"提取+分配"策略失败时（返回空或卡片数 < 规划的 30%），自动 fallback 到旧 9 次独立调用策略，并记录 `compile_diagnostics.md` | `[EXEC]` Mock 新策略失败验证 fallback 触发 |
| AC-PF-003 | **Map-Reduce 质量过滤性能**：`filter_cards_with_source` 只处理 chunk 级别的源文本（≤ 120K 字符），不处理完整 document。对于 500K 字符的文档，质量过滤总耗时提升 10 倍以上 | `[TEST]` 性能基准测试 |
| AC-PF-004 | **正则预编译**：`extract_field` 函数不再每次调用 `Regex::new`。所有正则使用 `LazyLock` 预编译 | `[INSP]` 代码审查 |
| AC-PF-005 | **StageCache 哈希缓存**：同一文档的多次 `cache_key` 计算只做一次 FNV 哈希，后续从缓存读取 | `[INSP]` 代码审查 `CompileContext.document_hash` |
| AC-PF-006 | **CardPlanner 多级 scale**：文档字符数 → scale 映射：≤5万→1, 5-15万→2, 15-30万→3, 30-50万→4, >50万→按每10万+1。scale 对所有卡片类型的 min/max 数量生效 | `[TEST]` 单元测试各字符数区间的 scale 值 |

---

## 七、可配置性 — 4 项

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-CF-001 | **书籍配置外部化**：`.cardnote/books.json` 文件存在时，`KNOWN_BOOKS` 从此文件加载。配置格式：`[{"name": "人生模式", "aliases": ["人生模式", "MOKA"], "author": "阳志平"}]`。新增书籍只需修改此文件，不重新编译 | `[EXEC]` 添加新书到 JSON，运行编译验证 ref 修复使用新配置 |
| AC-CF-002 | **并发数可配置**：`--max-workers` CLI 参数和 `CARDNOTE_MAX_WORKERS` 环境变量均能控制 Map-Reduce 并发数。默认值 = 2 | `[TEST]` 分别通过 CLI 参数和环境变量设置并发数=4，验证生效 |
| AC-CF-003 | **RPM 限制可配置**：`CARDNOTE_MAX_RPM` 环境变量控制 API 请求速率限制。不设置时不限速 | `[TEST]` 设置 RPM=2，验证每分钟请求 ≤ 2 |
| AC-CF-004 | **信息密度标记词可配置**：`.cardnote/density_markers.toml` 文件存在时，标记词从此文件加载而非使用硬编码默认值 | `[EXEC]` 修改配置文件中的标记词列表，运行验证信息密度计算使用新配置 |

---

## 八、批处理与状态管理 — 5 项

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-BT-001 | **批量处理 CLI**：`cardc batch <目录路径>` 扫描目录中所有 PDF，逐个处理。支持 `--output <目录>` 指定输出目录 | `[EXEC]` 对含 10 个 PDF 的目录运行 batch 命令验证 |
| AC-BT-002 | **断点续传**：`cardc batch <目录> --resume` 从上次中断位置继续，不重复处理已完成的 PDF。作业状态持久化在 `.cardnote/batch.db` SQLite 数据库中 | `[EXEC]` 处理到第 5 个时 kill 进程，`--resume` 验证从第 5 个继续 |
| AC-BT-003 | **失败重试**：`cardc batch <目录> --retry-failed` 只重试状态为 `Failed` 的作业，不重新处理 `Completed` 的作业 | `[EXEC]` 构造 3 个 Completed + 2 个 Failed，验证只处理 2 个 |
| AC-BT-004 | **进度查看**：`cardc batch-status` 显示 Pending / Running / Completed / Failed 各状态的作业数量 | `[EXEC]` 运行中执行 batch-status 验证输出 |
| AC-BT-005 | **用量持久化**：每次 LLM 调用后，用量数据（timestamp、model、input_tokens、output_tokens、cost）追加写入 `.cardnote/usage.log`（JSON Lines 格式）。`cardc usage-summary` 子命令输出历史总量统计 | `[EXEC]` 运行 5 本书后执行 usage-summary 验证 |

---

## 九、基础设施与运维 — 4 项

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-IO-001 | **临时文件自动清理**：启动时清理系统 temp 目录中创建时间超过 24 小时的 cardnote 相关临时目录（命名模式 `.tmp*` 或 `cardnote_*`） | `[EXEC]` 手动创建过期临时目录，启动程序后验证被清理 |
| AC-IO-002 | **磁盘残留上限**：连续处理 500 个 PDF 后（模拟中途 kill），磁盘新增残留文件总大小 < 100MB | `[EXEC]` 500 次运行后 `du -sh` 验证 |
| AC-IO-003 | **缓存清理不阻塞**：`Pipeline::new()` 初始化时，缓存清理在后台异步执行，不阻塞主流程。启动耗时增加 < 50ms（与无缓存清理相比） | `[TEST]` 性能基准测试 |
| AC-IO-004 | **Python 版本检查**：启动时检查必需的 Python 包（marker-pdf ≥ 1.0、pdfplumber ≥ 0.10）是否安装。缺失时输出精确的安装命令（如 `uv pip install -U 'marker-pdf>=1.0'`），而非运行时崩溃 | `[EXEC]` 在无依赖的环境中启动验证错误信息 |

---

## 十、代码工程债务 — 10 项

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-CD-001 | **无死代码**：`Anthropic`/`Gemini`/`Cohere` 协议相关代码标记 `#[allow(dead_code)]` 或移至 `unsupported_providers` 模块。`cargo clippy` 无 dead_code warning | `[EXEC]` `cargo clippy -- -D dead_code` 零 warning |
| AC-CD-002 | **ProviderRegistry 单例**：`ProviderRegistry::global()` 返回全局单例，不会每次调用 `new()` 重建 16 个 Provider | `[INSP]` 代码审查 `GLOBAL_REGISTRY: OnceLock<ProviderRegistry>` |
| AC-CD-003 | **scan_directory 无重复**：`scan_directory` 和 `scan_directory_async` 的公共逻辑提取到一个共享函数，代码重复行数 < 10 行 | `[INSP]` 代码审查 |
| AC-CD-004 | **CompileConfig 已激活或删除**：`CompileConfig` 在 Pipeline 中实际使用，或已从代码中移除。不存在"定义了但从未使用"的结构体 | `[INSP]` 代码审查 + `cargo clippy` |
| AC-CD-005 | **BookCompilationResult 已删除或使用**：`BookCompilationResult` 在批处理模块中实际使用，或已移除 | `[INSP]` 代码审查 |
| AC-CD-006 | **文件大小上限合理**：`MAX_FILE_SIZE_MB` 调整为 100MB（500MB 在转换层可能先 OOM）。如果确实需要处理更大文件，走 PDF 拆分路径 | `[INSP]` 代码审查 `config.rs` |
| AC-CD-007 | **resolve_book_title 统一入口**：书名解析只有一个入口函数（`converter.rs` 中的 `extract_pdf_metadata`）。`main.rs` 中不再有重复的书名解析逻辑 | `[INSP]` 代码审查 |
| AC-CD-008 | **去重算法 O(n²) 记录为债务**：当前算法保留，但在 `dedup.rs` 顶部注释中明确标注"单文档场景 O(n²) 可用，跨文档策展需升级为 MinHash/LSH" | `[INSP]` 代码审查注释 |
| AC-CD-009 | **cargo clippy 零 warning**：`cargo clippy --all-features` 返回 0 warning | `[EXEC]` CI 验证 |
| AC-CD-010 | **cargo fmt 检查通过**：`cargo fmt --check` 返回无差异 | `[EXEC]` CI 验证 |

---

## 十一、测试覆盖 — 6 项

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-TS-001 | **LCS 对抗性测试**：至少 5 个测试用例覆盖 LCS 相似度的边界情况（字符相同语序不同、完全相同、完全不同、空字符串、特殊字符） | `[TEST]` `cargo test` |
| AC-TS-002 | **自适应 Jaccard 对抗性测试**：至少 5 个测试用例覆盖短内容去重的边界情况（语义相近措辞不同、完全相同、完全不同、短内容自适应配置切换） | `[TEST]` `cargo test` |
| AC-TS-003 | **质量评分区分度测试**：至少 3 个测试用例验证不同 issue 组合的评分有区分度 | `[TEST]` `cargo test` |
| AC-TS-004 | **ref 格式校验测试**：至少 8 个测试用例——有效格式（`书名_p数字`、`书名_p数字-数字`）+ 六种无效格式（书名号、本书、章节名、前导零、作者名、空 ref） | `[TEST]` `cargo test` |
| AC-TS-005 | **质量门控拦截测试**：验证带 reject_reason 的卡片不进入输出；status != Accepted 的卡片不进入输出 | `[TEST]` `cargo test` |
| AC-TS-006 | **端到端集成测试**：对一个已知内容的小 PDF（≤ 5 页）运行完整编译流程，验证程序不 panic 且有卡片产出 | `[TEST]` CI 集成测试 |

---

## 十二、文档与展示（1000 Star 关键）— 8 项

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-DC-001 | **README 有卡片示例截图**：README 中包含至少 1 张"PDF 原文段落截图 vs 生成的对应卡片截图"的 before/after 对比 | `[INSP]` 查看 GitHub README |
| AC-DC-002 | **README 有 GIF 演示**：README 顶部有 30 秒以内的 GIF/WebM 动画演示（一本书的完整编译流程：`cardc book.pdf`） | `[INSP]` 查看 GitHub README |
| AC-DC-003 | **README 有架构图**：README 包含系统架构图（使用 Mermaid 或图片），清晰展示 PDF→Converter→Pipeline→Cards 的数据流 | `[INSP]` 查看 GitHub README |
| AC-DC-004 | **README 有一行安装命令**：`cargo install cardnote-compiler` 或等效的一行安装方式 | `[EXEC]` 在新环境中执行安装命令验证成功 |
| AC-DC-005 | **README 有一行使用命令**：`cardc ./book.pdf` 即可开始使用，无需额外配置（API Key 除外） | `[EXEC]` 执行验证 |
| AC-DC-006 | **CONTRIBUTING.md 存在**：包含如何贡献代码/测试/prompt 的说明 | `[INSP]` 文件存在且内容完整 |
| AC-DC-007 | **CHANGELOG.md 存在**：遵循 Keep a Changelog 格式，记录每个版本的 Added / Changed / Fixed / Removed | `[INSP]` 文件存在且格式符合规范 |
| AC-DC-008 | **所有公开 API 有 rustdoc**：`cargo doc --no-deps` 无 warning。核心公共函数有文档注释 | `[EXEC]` `cargo doc --no-deps 2>&1 | grep -c warning` = 0 |

---

## 十三、CI/CD — 3 项

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-CI-001 | **GitHub Actions 自动测试**：push 到 main/master 分支时自动运行 `cargo test --all-features` | `[INSP]` `.github/workflows/ci.yml` 文件存在且工作流正确 |
| AC-CI-002 | **GitHub Actions 自动 lint**：push 时自动运行 `cargo clippy -- -D warnings` 和 `cargo fmt --check` | `[INSP]` CI 配置文件 |
| AC-CI-003 | **CI 缓存 Rust 依赖**：使用 `Swatinem/rust-cache@v2` 加速 CI 构建 | `[INSP]` CI 配置文件 |

---

## 十四、Markdown 输出格式 — 3 项

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-MD-001 | **Markdown 转义**：卡片内容中的 `---`、`#`、`*`、`_`、`>` 等 Markdown 特殊字符在输出前做转义处理。一根 `---` 不会误解析为卡片分隔符 | `[TEST]` 构造含特殊字符的卡片内容，验证输出 Markdown 解析正确 |
| AC-MD-002 | **卡片分隔符一致**：所有卡片之间使用三个或以上 `---` 作为分隔符，分隔符格式统一 | `[TEST]` 正则验证输出文件格式 |
| AC-MD-003 | **中文排版修复自动应用**：输出前自动执行 `typo_fix()`（中西文间距、标点挤压、直角引号等），不依赖外部 42md 工具 | `[TEST]` 验证输出中的中英文间距正确 |

---

## 十五、Chunk 边界处理 — 2 项

| # | 验收标准 | 验证方法 |
|---|---------|---------|
| AC-CK-001 | **语义边界优先切分**：分块时优先在 Markdown 标题（`^#{1,3}\s+`）处切分，其次在段落边界处切分，不硬按字符数截断 | `[INSP]` 代码审查 `smart_chunk_split` 函数 |
| AC-CK-002 | **Chunk 间重叠上下文**：每个 chunk 的末尾和下一个 chunk 的开头保留 2,000 字符重叠上下文（当前已有），确保横跨边界的概念不丢失 | `[INSP]` 代码审查 Pipeline 分块逻辑 |

---

## 总验收矩阵

| 章节 | 验收项数 | 关键项（阻断发布） |
|------|---------|------------------|
| 一、卡片内容质量 | 19 | AC-P-001 ~ AC-P-011, AC-P-015 |
| 二、校验算法正确性 | 15 | AC-V-001 ~ AC-V-015（全部） |
| 三、质量门控 | 5 | AC-QG-001, AC-QG-002（全部） |
| 四、数据完整性与安全 | 5 | AC-DI-001 ~ AC-DI-005（全部） |
| 五、可靠性 | 6 | AC-RL-001 ~ AC-RL-006（全部） |
| 六、性能与成本 | 6 | AC-PF-001, AC-PF-002 |
| 七、可配置性 | 4 | AC-CF-001, AC-CF-002 |
| 八、批处理与状态管理 | 5 | AC-BT-001, AC-BT-002 |
| 九、基础设施与运维 | 4 | AC-IO-001, AC-IO-004 |
| 十、代码工程债务 | 10 | AC-CD-001, AC-CD-009 |
| 十一、测试覆盖 | 6 | AC-TS-001 ~ AC-TS-003 |
| 十二、文档与展示 | 8 | AC-DC-001 ~ AC-DC-005 |
| 十三、CI/CD | 3 | AC-CI-001 |
| 十四、Markdown 输出 | 3 | AC-MD-001 |
| 十五、Chunk 边界 | 2 | AC-CK-001 |
| **总计** | **106** | **—** |

---

## 通过标准

- **100% 关键项（标"全部"的类别）必须通过**——任何一项不通过，不得发布 v1.0
- **非关键项通过率 ≥ 95%**（106 项中允许最多 5 项因合理原因暂缓，但必须在 CHANGELOG 中标注为 "Known Limitations"）
- 暂缓项不得超过 5 项，且不得包含任何"数据丢失/安全/质量门控"类问题

---

*本验收标准覆盖 card-quality-audit、code_review_v4、comprehensive-meta-analysis、grill-me-decisions、ULTIMATE_FIX_BLUEPRINT 五份报告的全部 54 项问题 + 元反空分析发现的额外遗漏。零遗漏。*
