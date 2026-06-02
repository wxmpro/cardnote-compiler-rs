# CardNote Compiler

把 PDF 编译成结构化知识卡片。支持单文件和批量处理，内置 12 种卡片类型。

**当前版本**：v0.1.35

## 快速开始

```bash
# 安装
cargo build --release
ln -s $(pwd)/target/release/cardc ~/.local/bin/cardc

# 配置 API
cardc init

# 编译一本书
cardc ./book.pdf

# 批量处理
cardc batch ./pdfs/
```

## 核心特性

- **Extract-Then-Assign**：短文档自动使用 2 次 LLM 调用（vs 传统 9 次），API 成本降 60%，失败时自动回退
- **12 种卡片类型**：术语卡、新知卡、反常识卡、金句卡、人物卡、事件卡、行动卡、图示卡、新词卡、综述卡、基础卡、索引卡
- **批量处理**：`cardc batch` 一键处理整个目录，SQLite 作业队列支持断点续传和失败重试
- **智能分块**：长文档自动触发 Map-Reduce 语义分块编译，块间保留上下文重叠
- **质量门控**：自动检测标题与内容不匹配、ref 格式违规、LLM 编造例子、类型混淆等问题，拦截不输出
- **16 家 LLM 提供商**：内置支持，自动健康检测与智能选择
- **编译缓存**：Stage 级缓存 + 文档哈希缓存，断点续编译
- **RPM 限流**：`CARDNOTE_MAX_RPM` 环境变量控制 API 调用频率
- **多格式输入**：PDF（文字层/扫描版 OCR）、Word、EPUB、Markdown、HTML 等
- **可配置 Prompt**：所有卡片类型 prompt 独立可编辑，书籍列表运行时加载

## 命令一览

| 命令 | 用途 |
|------|------|
| `cardc <文件>` | 完整编译（自动选择策略） |
| `cardc batch <目录>` | 批量处理，支持 `--resume` / `--retry-failed` |
| `cardc batch-status` | 查看批量处理进度 |
| `cardc scan <目录>` | PDF 预扫描（检测 OCR 需求） |
| `cardc quality <文件>` | 输入质量检测 |
| `cardc summary <文件>` | 仅 AI 摘要 |
| `cardc cards <文件>` | 仅生成卡片 |
| `cardc graph <文件>` | 仅生成关系图谱 |
| `cardc doctor` | 环境诊断 |
| `cardc init` | 重新配置 API |

## 环境要求

| 组件 | 版本 | 用途 |
|------|------|------|
| Rust | 1.85+ | 编译工具 |
| Python | 3.11+ | PDF 解析（推荐） |
| pymupdf | 最新 | PDF 文本提取 |
| markitdown | 最新 | 通用文档转换 |
| MinerU | 最新 | 扫描版 PDF OCR（可选） |

## 配置

```bash
# .env 基本配置
LLM_API_KEY=sk-your-key
LLM_PROVIDER=deepseek
LLM_MODEL=deepseek-chat

# 阶段级模型（可选）
LLM_MODEL_SUMMARY=kimi-2.6
LLM_MODEL_CARDS=deepseek-chat

# RPM 限流（可选）
CARDNOTE_MAX_RPM=30

# Map-Reduce 并发（可选，默认 2）
CARDNOTE_MAX_WORKERS=4

# 自定义书籍列表（可选）
# .cardnote/books.json
[{"name": "书名", "aliases": ["简称"], "author": "作者"}]

# 自定义密度标记词（可选）
# .cardnote/density_markers.toml
```

## 编译策略

```
输入文档
  ↓
[PDF 探测] → 扫描版？→ OCR 处理
  ↓
[质量检测] → 过低？→ 中止（或 --force）
  ↓
[策略选择]
  ├── ≤200K 字符 → Extract-Then-Assign（2 次调用）
  │   └── 失败/产出不足 → 自动回退分类型策略（9 次调用）
  └── >200K 字符 → Map-Reduce 语义分块编译
  ↓
[质量门控] → reject_reason 非空或 status≠Accepted 的卡片不输出
  ↓
[输出] → Markdown 卡片 + 知识图谱 + 质量报告
```

## 项目结构

```
src/
├── main.rs          # CLI 入口（clap）
├── pipeline.rs      # 编译流水线 + Stage 缓存
├── api.rs           # LLM 客户端 + JSON 降级 + 用量持久化
├── stages/cards.rs  # 卡片生成（Extract-Then-Assign + legacy fallback）
├── stages/          # summary / entities / graph
├── batch.rs         # 批量处理（SQLite 作业队列）
├── rate_limiter.rs  # Token-bucket 限流器
├── converter.rs     # PDF → Markdown（多层 fallback）
├── quality/         # 质量系统（card_lint / typo_lint / preprocess）
├── dedup.rs         # 语义去重（自适应 Jaccard + LCS）
├── config.rs        # 配置管理（books / markers / RPM）
├── providers.rs     # 16 家 LLM 提供商注册表
├── scan.rs          # PDF 预扫描
├── models.rs        # 数据模型
└── output.rs        # 结果输出 + 质量报告
prompts/             # Prompt 模板（12 种卡片类型 + 提取/分配/摘要等）
```

## 常见问题

**编译报错 Python 依赖未找到**：`uv pip install pymupdf markitdown`

**API Key 无效**：`cardc doctor` 诊断 → `cardc init` 重新配置

**扫描版 PDF 超时**：`cardc scan ./` 检测 → OCR 处理后重试

**输出质量差**：检查 `card_quality_report.md`，ref 格式/例子真实性/类型混淆均有自动检测

## 发布边界

以下内容不入仓库：API Key（`.env`）、用户 PDF/卡片输出、缓存（`.cardc_cache/`）、编译产物（`target/`）
