# CardNote Compiler

> **版本**: v0.1.49  
> **二进制名**: `cardc`  
> **定位**: 将文档（PDF / Word / EPUB / Markdown / HTML 等）编译为结构化知识卡片的 CLI 工具。Rust 实现，异步流水线，LLM 驱动。

---

## 一、项目定位与适用边界

### 1.1 项目做什么

CardNote Compiler 读取输入文档，通过 LLM 多阶段流水线将其转化为一套结构化知识产物：

| 产物 | 说明 |
|------|------|
| **AI 摘要** | 书名、概述、核心要点、结构分析 |
| **知识卡片** | 12 种类型卡片（术语卡、新知卡、反常识卡、金句卡、人物卡、事件卡、行动卡、图示卡、新词卡、综述卡、基础卡、索引卡） |
| **实体图谱** | 文档中的关键实体及其关系网络（Mermaid 格式） |
| **质量报告** | 输入质量评分、卡片质量检测、Lint 报告 |

### 1.2 适用条件（什么情况下用）

| 条件 | 要求 |
|------|------|
| **输入格式** | PDF（文字层/扫描版）、Word（.doc/.docx）、EPUB、Markdown、HTML、纯文本 |
| **输入质量** | 文字层 PDF 可直接处理；扫描版/图片版 PDF 需先 OCR（工具链内置检测与提示） |
| **LLM 接入** | 必须配置至少一个 LLM Provider 的 API Key（详见「配置」章节） |
| **文档语言** | 中文为主，英文支持（Prompt 模板为中英双语架构，当前默认加载中文版本） |
| **文档长度** | ≤ 20 万字符：Extract-Then-Assign 策略（2 次 LLM 调用）；> 20 万字符：Map-Reduce 语义分块编译 |
| **系统环境** | macOS / Linux / Windows；Rust 1.85+；Python 3.11+（PDF 解析辅助） |

### 1.3 不适用场景（什么情况下不要用）

- **手写笔记/照片**：非结构化图像，无 OCR 预处理时无法处理
- **表格/数据密集型 PDF**：当前流水线针对叙事性文本优化，表格抽取质量有限
- **多语言混排文档**：实体识别和卡片分类可能混淆
- **无 API Key 环境**：所有卡片生成功能依赖 LLM，离线不可用

---

## 二、核心特性

- **Extract-Then-Assign 策略**（短文档 ≤ 20 万字符）：先一次性提取全部知识块，再统一分配卡片类型。API 调用从传统 9 次降至 2 次，成本降低约 60%。失败时自动回退到分类型策略。
- **Map-Reduce 语义分块**（长文档 > 20 万字符）：按语义边界分块，块间保留上下文重叠，逐块编译后汇总。
- **质量门控系统**：自动拦截标题与内容不匹配、ref 格式违规、LLM 编造例子、类型混淆等问题的卡片，拒绝输出。
- **自动 Provider 健康检测**：启动时自动检测所有配置 Provider 的连通性与延迟，选择最优可用者。
- **编译历史追踪**：SQLite 持久化每次编译记录，支持 `history` / `review` 命令查看与审阅。
- **Prompt 完全可编辑**：所有卡片类型、提取/分配/摘要/图谱的 Prompt 均为独立 `.md` 文件，用户可直接修改。
- **Stage 级缓存**：编译中断后重新运行，已完成的阶段自动跳过。

---

## 三、项目结构

```
cardnote-compiler-rs/
├── Cargo.toml              # Rust 项目配置
├── Cargo.lock              # 依赖锁定
├── CHANGELOG.md            # 版本变更记录
├── README.md               # 本文档
├── .env                    # 环境变量（API Key 等，不提交 Git）
├── .cardnote/
│   └── providers.json      # LLM Provider 统一配置（推荐配置方式）
├── documents/              # 存放原始文档（自动复制，命名规则：{日期}_{文档名}）
│   └── README.md
├── output/                 # 默认输出目录
├── prompts/                # Prompt 模板（24 个文件，全部可编辑）
│   ├── action_card.md      # 行动卡
│   ├── counter_intuit_card.md  # 反常识卡
│   ├── entity_extraction.md    # 实体提取
│   ├── event_card.md       # 事件卡
│   ├── free_card.md        # 自由卡
│   ├── graph_card.md       # 图示卡
│   ├── index_card.md       # 索引卡
│   ├── knowledge_card.md   # 新知卡
│   ├── new_word_card.md    # 新词卡
│   ├── note_card.md        # 基础卡
│   ├── person_card.md      # 人物卡
│   ├── quality_check.md    # 质量检查
│   ├── quote_card.md       # 金句卡
│   ├── relation_graph.md   # 关系图谱
│   ├── summary.md          # 摘要生成
│   ├── technique_card.md   # 技巧卡
│   ├── term_card.md        # 术语卡
│   └── ...（其余分配/提取/检查类 Prompt）
├── scripts/
│   └── paddle_ocr_pdf.py   # PaddleOCR 辅助脚本（扫描版 PDF）
├── src/
│   ├── main.rs             # CLI 入口（clap 参数解析 + 命令路由）
│   ├── lib.rs              # 库入口（模块声明）
│   ├── pipeline.rs         # 编译流水线 + Stage 缓存 + 重试逻辑
│   ├── api.rs              # LLM HTTP 客户端 + JSON 降级 + 用量统计
│   ├── converter.rs        # 文档 → Markdown 转换（多层 fallback）
│   ├── models.rs           # 核心数据结构（Card / Summary / Entity / Relation 等）
│   ├── config.rs           # 配置管理（Provider / 书籍列表 / 密度标记词 / 限流）
│   ├── providers.rs        # LLM 提供商注册表（16 家内置）
│   ├── health.rs           # Provider 健康检测与自动选择
│   ├── rate_limiter.rs     # Token-bucket RPM 限流器
│   ├── scan.rs             # PDF 预扫描（文字层 / OCR 需求检测）
│   ├── dedup.rs            # 语义去重（自适应 Jaccard + LCS）
│   ├── batch.rs            # SQLite 编译记录持久化（history / review 数据来源）
│   ├── diagnostics.rs      # 环境诊断（doctor 命令实现）
│   ├── error.rs            # 错误类型定义
│   ├── doc_type.rs         # 文档类型检测
│   ├── output.rs           # 结果输出（Markdown / 质量报告 / 目录组织）
│   ├── stages/
│   │   ├── mod.rs          # 阶段模块入口
│   │   ├── cards.rs        # 卡片生成（Extract-Then-Assign + fallback）
│   │   ├── common.rs       # 阶段共享逻辑
│   │   ├── entities.rs     # 实体提取阶段
│   │   ├── graph.rs        # 关系图谱阶段
│   │   └── summary.rs      # 摘要生成阶段
│   └── quality/
│       ├── mod.rs          # 质量系统入口
│       ├── preprocess.rs   # 输入预处理
│       ├── card_lint.rs    # 卡片 Lint（ref 格式 / 类型检查 / 自动修复）
│       ├── typo_lint.rs    # 拼写检查
│       ├── metrics.rs      # 质量评分
│       ├── report.rs       # 质量报告生成
│       └── pdf_postprocess.rs  # PDF 后处理
└── tests/                  # 测试与审查报告
    ├── en_prompts_v3/      # 英文 Prompt 备份
    ├── prompts_backup/     # 历史 Prompt 备份
    ├── v3_prompts/         # 当前中文 Prompt 备份
    └── review_reports/     # 代码审查与审计报告
```

---

## 四、安装

### 4.1 前置依赖

| 组件 | 版本 | 用途 |
|------|------|------|
| Rust | 1.85+ | 编译本工具 |
| Python | 3.11+ | PDF 解析辅助 |
| pymupdf | 最新 | PDF 文本提取（强烈推荐） |
| markitdown | 最新 | 通用文档转换 |
| PaddleOCR（可选） | 最新 | 扫描版 PDF OCR |

```bash
# 安装 Python 依赖
uv pip install pymupdf markitdown
# 如需 OCR 支持
uv pip install paddlepaddle paddleocr
```

### 4.2 编译安装

```bash
# 克隆仓库后
cd cardnote-compiler-rs

# 编译 Release 版本
cargo build --release

# 创建软链接到 PATH（推荐）
ln -s $(pwd)/target/release/cardc ~/.local/bin/cardc
# 或复制
cp target/release/cardc ~/.local/bin/

# 验证安装
cardc --version
```

---

## 五、配置

### 5.1 推荐方式：`~/.config/cardnote/providers.json`

首次运行 `cardc init` 或任意编译命令时，工具会自动导出内置 Provider 配置到 `~/.config/cardnote/providers.default.json`。用户只需创建 `providers.json` 写入需覆盖的 Provider：

```json
{
  "_comment": "CardNote Compiler 统一配置 — 只需写入要使用的 Provider",
  "deepseek": {
    "api_key": "sk-your-key-here",
    "base_url": "https://api.deepseek.com/v1",
    "model": "deepseek-chat"
  },
  "openai": {
    "api_key": "sk-your-openai-key",
    "model": "gpt-4o"
  }
}
```

配置加载优先级（从高到低）：
1. 命令行参数（`--provider` / `--model` / `--api-key` / `--base-url`）
2. `~/.config/cardnote/providers.json`（用户自定义）
3. `~/.config/cardnote/providers.default.json`（自动导出）
4. 内置默认值

### 5.2 环境变量方式（备用）

在项目根目录创建 `.env` 文件：

```bash
# 基本配置
LLM_API_KEY=sk-your-key
LLM_PROVIDER=deepseek
LLM_MODEL=deepseek-chat

# 阶段级模型分配（可选，不同阶段用不同模型）
LLM_MODEL_SUMMARY=kimi-2.6
LLM_MODEL_CARDS=deepseek-chat

# RPM 限流（可选，默认 30）
CARDNOTE_MAX_RPM=30

# Map-Reduce 并发数（可选，默认 2）
CARDNOTE_MAX_WORKERS=4
```

### 5.3 自定义书籍列表（可选）

创建 `.cardnote/books.json`：

```json
[
  {
    "name": "人生模式：识别并优化你的核心认知",
    "aliases": ["人生模式"],
    "author": "阳志平"
  }
]
```

用于提升 ref 格式中书名提取的准确性。

### 5.4 自定义密度标记词（可选）

创建 `.cardnote/density_markers.toml`，定义文本密度判定阈值用词。

---

## 六、CLI 命令详解

### 命令概览

```
cardc [全局选项] <文件>              # 完整编译（默认命令）
cardc [全局选项] <子命令> [参数]      # 子命令模式
```

### 全局选项

| 选项 | 简写 | 默认值 | 说明 |
|------|------|--------|------|
| `--output` | `-o` | `./output` | 输出目录 |
| `--provider` | — | — | LLM 提供商 ID（如 deepseek / openai） |
| `--model` | — | — | 模型名称（如 deepseek-chat / gpt-4o） |
| `--api-key` | — | — | API Key（⚠️ 会留在 shell history，建议用环境变量） |
| `--base-url` | — | — | 自定义 API 地址 |
| `--force` | — | — | 低质量输入仍强制继续编译 |
| `--version` | `-V` | — | 显示版本号 |
| `--help` | `-h` | — | 显示帮助 |

---

### 6.1 `cardc <文件>` — 完整编译（默认命令）

**用途**: 执行完整编译流水线：PDF 探测 → 质量检测 → 策略选择 → 摘要 → 实体 → 卡片 → 图谱 → 质量门控 → 输出保存。

**参数**:
- `文件` — 输入文件路径（PDF / Word / EPUB / Markdown / HTML / 文本）

**选项**: 支持全部全局选项。

**示例**:

```bash
# 基本用法
cardc ./人生模式.pdf

# 指定输出目录
cardc ./人生模式.pdf -o ./output/人生模式

# 指定 Provider 和模型
cardc ./人生模式.pdf --provider deepseek --model deepseek-chat

# 强制编译（跳过质量检查）
cardc ./扫描版书籍.pdf --force
```

**执行流程**:

```
输入文件
  ↓
[Provider 健康检测] → 自动选择最优可用 Provider
  ↓
[PDF 探测] → 检测文字层 / 扫描版 / 手绘
  ├── 需 OCR 且未 --force → 中止，保存质量报告
  └── 文字层 / --force → 继续
  ↓
[文档转换] → 转换为 Markdown
  ↓
[输入质量检测] → 评分（A/B/C/D/F）
  ├── < C 且未 --force → 中止
  └── ≥ C 或 --force → 继续
  ↓
[策略选择]
  ├── ≤ 200,000 字符 → Extract-Then-Assign（2 次 LLM）
  │   └── 失败/产出不足 → 自动回退分类型策略（多次调用）
  └── > 200,000 字符 → Map-Reduce 语义分块
  ↓
[流水线执行]
  ├── 摘要阶段（summary）
  ├── 实体阶段（entities）
  ├── 卡片阶段（cards）
  └── 图谱阶段（graph）
  ↓
[质量门控] → 拦截 reject_reason 非空或 status≠Accepted 的卡片
  ↓
[输出保存] → Markdown 卡片 + 知识图谱 + 质量报告 + 输入质量报告
  ↓
[记录持久化] → SQLite 编译记录（供 history 命令查看）
```

**输出目录结构**:

```
output/
└── {日期}_{书名}/
    ├── README.md                 # 总览（摘要 + 卡片列表）
    ├── all_cards.md              # 全部卡片合并
    ├── summary.md                # AI 摘要
    ├── entities.md               # 实体列表
    ├── graph.mmd                 # Mermaid 关系图谱
    ├── card_quality_report.md    # 卡片质量报告
    ├── input_quality_report.md   # 输入质量报告
    ├── compile_diagnostics.md    # 编译诊断（如有失败）
    └── cards/
        ├── 术语卡.md
        ├── 新知卡.md
        ├── 反常识卡.md
        ├── 金句卡.md
        ├── 人物卡.md
        ├── 事件卡.md
        ├── 行动卡.md
        ├── 图示卡.md
        ├── 新词卡.md
        ├── 综述卡.md
        ├── 基础卡.md
        └── 索引卡.md
```

---

### 6.2 `cardc summary <文件>` — 仅生成 AI 摘要

**用途**: 只执行摘要阶段，输出到终端，不生成卡片和图谱。

**参数**:
- `文件` — 输入文件路径

**选项**:
- `-o, --output <目录>` — 输出目录（默认 `./output`）

**示例**:

```bash
cardc summary ./人生模式.pdf
cardc summary ./人生模式.pdf -o ./summaries
```

---

### 6.3 `cardc annotate <文件>` — 仅运行 AI 实体标注

**用途**: 只执行实体提取阶段，输出实体列表到终端。

**参数**:
- `文件` — 输入文件路径

**示例**:

```bash
cardc annotate ./人生模式.pdf
# 输出示例：
# - 阳志平 (人物)
# - 人生模式 (概念)
# - 核心认知 (概念)
```

---

### 6.4 `cardc cards <文件>` — 仅生成知识卡片

**用途**: 只执行卡片生成阶段，按类型保存到 `cards/` 目录。

**参数**:
- `文件` — 输入文件路径

**选项**:
- `-o, --output <目录>` — 输出目录（默认 `./output`）

**示例**:

```bash
cardc cards ./人生模式.pdf -o ./output/卡片
```

---

### 6.5 `cardc graph <文件>` — 仅生成关系图谱

**用途**: 只执行实体提取 + 关系图谱阶段，输出 Mermaid 语法到终端。

**参数**:
- `文件` — 输入文件路径

**示例**:

```bash
cardc graph ./人生模式.pdf
# 输出 Mermaid 语法，可直接粘贴到支持 Mermaid 的编辑器
```

---

### 6.6 `cardc init` — 交互式配置

**用途**: 重新配置 API 设置。引导式交互，选择 Provider、输入 API Key、选择模型。

**示例**:

```bash
cardc init
```

---

### 6.7 `cardc doctor` — 环境诊断

**用途**: 全面检查环境：LLM Provider 连通性、Python 依赖、PDF 工具链、Prompt 文件完整性、配置有效性。

**示例**:

```bash
cardc doctor
```

---

### 6.8 `cardc quality <文件>` — 输入质量检测

**用途**: 检测输入文档的解析质量，输出评分报告（不调用 LLM，纯本地分析）。

**参数**:
- `文件` — 输入文件路径

**示例**:

```bash
cardc quality ./人生模式.pdf
# 输出：
# 输入质量: A (92.3/100)
# - 文本密度: 高
# - 乱码率: 0.1%
# - 完整度: 98.5%
```

---

### 6.9 `cardc scan <目录>` — 扫描 PDF OCR 需求

**用途**: 批量扫描目录下的 PDF，检测哪些需要 OCR 处理。

**参数**:
- `目录` — 要扫描的目录路径

**选项**:
- `-r, --recursive` — 递归扫描子目录
- `-t, --threshold <字符数>` — 文字层判定阈值（默认 20 字符）

**示例**:

```bash
# 扫描当前目录
cardc scan ./

# 递归扫描
cardc scan ./pdfs/ --recursive

# 自定义阈值
cardc scan ./pdfs/ -t 50
```

---

### 6.10 `cardc history` — 查看编译历史

**用途**: 查看 SQLite 中持久化的编译记录。

**选项**:
- `-l, --limit <数量>` — 显示最近 N 条记录（默认 20）

**示例**:

```bash
cardc history
cardc history --limit 50
```

**输出示例**:

```
╔══════════════════════════════════════════════════════════════╗
║               CardNote 编译历史                              ║
╠══════════════════════════════════════════════════════════════╣
║  15 本书 | 32 次编译 | 1,247 张卡片 | 待审阅 3 本           ║
║  累计 2,456,789 tokens                                      ║
╚══════════════════════════════════════════════════════════════╝

  ✓ v0.1.49 2026-06-01 14:32  人生模式  → 45/3 张 (通过/拦截) | ./output/20260601_人生模式
  ○ v0.1.49 2026-06-01 10:15  聪明的阅读者 → 38/0 张 | ./output/20260601_聪明的阅读者
```

---

### 6.11 `cardc review <ID>` — 标记记录为已审阅

**用途**: 将指定编译记录标记为已审阅（ID 从 `cardc history` 获取）。

**参数**:
- `ID` — 编译记录 ID

**示例**:

```bash
cardc review 42
```

---

## 七、12 种卡片类型说明

| 卡片类型 | 用途 | Prompt 文件 |
|----------|------|-------------|
| **术语卡** | 专业术语 / 概念定义 | `prompts/term_card.md` |
| **新知卡** | 新知识 / 新方法 / 新观点 | `prompts/knowledge_card.md` |
| **反常识卡** | 与直觉相反但有证据支撑的观点 | `prompts/counter_intuit_card.md` |
| **金句卡** | 精辟论述 / 经典引用 | `prompts/quote_card.md` |
| **人物卡** | 关键人物及其贡献 | `prompts/person_card.md` |
| **事件卡** | 重要事件 / 时间节点 | `prompts/event_card.md` |
| **行动卡** | 可执行的建议 / 方法论 | `prompts/action_card.md` |
| **图示卡** | 需要可视化呈现的概念 / 流程 | `prompts/graph_card.md` |
| **新词卡** | 生僻词 / 外来词 / 黑话 | `prompts/new_word_card.md` |
| **综述卡** | 章节 / 主题综述 | `prompts/note_card.md` |
| **基础卡** | 背景知识 / 前提概念 | `prompts/note_card.md` |
| **索引卡** | 主题索引 / 导航 | `prompts/index_card.md` |

所有卡片统一包含以下字段：`title`（标题）、`content`（内容）、`ref`（引用，格式：`《书名》_p页码｜原句摘要`）、`type`（卡片类型）。

---

## 八、Prompt 自定义指南

所有 Prompt 位于 `prompts/` 目录，均为 Markdown 文件，可直接用文本编辑器修改。**修改后无需重新编译，下次运行自动生效。**

```bash
# 示例：自定义术语卡 Prompt
vim prompts/term_card.md

# 示例：自定义摘要 Prompt
vim prompts/summary.md
```

Prompt 加载规则：
- 编译时从 `prompts/` 目录按文件名加载
- 文件不存在时回退到内置默认 Prompt
- 修改 Prompt 不会影响已编译的历史输出

---

## 九、常见问题

### Q1: 编译报错 "Python 依赖未找到"

```bash
uv pip install pymupdf markitdown
```

### Q2: API Key 无效或 Provider 连接失败

```bash
cardc doctor          # 诊断问题
cardc init            # 重新配置
```

### Q3: 扫描版 PDF 编译超时或质量差

```bash
cardc scan ./         # 先检测哪些需要 OCR
# 对需 OCR 的文件，使用 PaddleOCR 预处理
python scripts/paddle_ocr_pdf.py ./扫描版.pdf
# 然后编译处理后的文本文件
cardc ./扫描版_ocr.txt
```

### Q4: 输出质量不理想

1. 检查 `input_quality_report.md` — 输入文本质量是否足够
2. 检查 `card_quality_report.md` — 哪些卡片被拦截及原因
3. 检查 `compile_diagnostics.md` — 是否有阶段失败
4. 尝试修改对应卡片类型的 Prompt

### Q5: 编译结果为空（0 张卡片）

可能原因：
- LLM 返回全部为空（检查 API Key 是否有效、余额是否充足）
- 输入文档过短或无实质内容
- 所有卡片被质量门控拦截

排查：
```bash
cardc doctor                  # 检查环境
cardc quality ./文件.pdf      # 检查输入质量
cat output/.../compile_diagnostics.md  # 查看诊断
```

---

## License

MIT
