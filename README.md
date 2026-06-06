# CardNote Compiler

把文档编译成结构化知识卡片。支持单文件处理，内置 12 种卡片类型。

**当前版本**：v0.1.59

## 快速开始

```bash
# 安装
cargo build --release
ln -s $(pwd)/target/release/cardc ~/.local/bin/cardc

# 配置 API（首次运行必需）
cardc init

# 编译一本书（Unified 模式：实体+卡片一次性生成）
cardc ./book.pdf
```

## 核心特性

- **Unified 编译**：一次 LLM 请求同时生成实体 + 卡片（vs 传统 4 次请求），API 成本降 70%
- **12 种卡片类型**：术语卡、新知卡、反常识卡、金句卡、人物卡、事件卡、行动卡、图示卡、新词卡、综述卡、基础卡、索引卡
- **智能分块**：根据模型上下文窗口自动计算分块阈值，支持 64K~1M tokens 的任意模型
- **动态卡片规划**：根据文档长度自动调整卡片数量（42万字书籍 → 80-160 张），不再受限于固定 10-20 张
- **质量门控**：自动检测标题与内容不匹配、ref 格式违规、LLM 编造例子、类型混淆等问题，拦截不输出
- **自动 Provider 选择**：启动时自动检测所有配置 Provider 的连通性，选择最优可用者
- **编译缓存**：文档哈希缓存，断点续编译
- **编译历史追踪**：SQLite 持久化每次编译记录，支持 `history` / `review` 命令查看与审阅
- **RPM 限流**：`CARDNOTE_MAX_RPM` 环境变量控制 API 调用频率
- **多格式输入**：PDF（文字层/扫描版 OCR）、Word、EPUB、Markdown、HTML 等
- **可配置 Prompt**：所有卡片类型 prompt 独立可编辑，修改后无需重新编译即可生效

## 命令一览

| 命令 | 用途 |
|------|------|
| `cardc <文件>` | 完整编译（自动选择策略） |
| `cardc init` | 交互式配置 API（首次运行必需） |
| `cardc scan <目录>` | PDF 预扫描（检测 OCR 需求） |
| `cardc quality <文件>` | 输入质量检测（不调用 LLM） |
| `cardc history` | 查看编译历史记录 |
| `cardc review <ID>` | 标记编译记录为已审阅 |
| `cardc doctor` | 环境诊断 |

### 全局选项

| 选项 | 简写 | 默认值 | 说明 |
|------|------|--------|------|
| `--output` | `-o` | `./output` | 输出目录 |
| `--provider` | — | — | LLM 提供商 ID |
| `--model` | — | — | 模型名称 |
| `--api-key` | — | — | API Key（⚠️ 会留在 shell history） |
| `--base-url` | — | — | 自定义 API 地址 |
| `--force` | — | — | 低质量输入仍强制继续编译 |

## 环境要求

| 组件 | 版本 | 用途 |
|------|------|------|
| Rust | 1.85+ | 编译工具 |
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

## 配置

### 推荐方式：`.cardnote/providers.json`

首次运行 `cardc init` 时，工具会自动导出内置 Provider 配置到 `~/.config/cardnote/providers.default.json`。用户只需创建 `providers.json` 写入需覆盖的 Provider：

```json
{
  "deepseek": {
    "api_key": "sk-your-key-here",
    "base_url": "https://api.deepseek.com/v1",
    "model": "deepseek-v4-pro"
  }
}
```

配置加载优先级：命令行参数 → `providers.json`（用户自定义）→ `providers.default.json`（自动导出）→ 内置默认值。

### 备用方式：`.env` 环境变量

```bash
LLM_API_KEY=sk-your-key
LLM_PROVIDER=deepseek
LLM_MODEL=deepseek-v4-pro

# RPM 限流（可选，默认 30）
CARDNOTE_MAX_RPM=30
```

### 自定义书籍列表（可选）

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

## 编译策略

```
输入文档
  ↓
[Provider 健康检测] → 自动选择最优可用 Provider
  ↓
[PDF 探测] → 扫描版？→ OCR 处理
  ↓
[质量检测] → 过低？→ 中止（或 --force）
  ↓
[策略选择] ← 根据模型上下文长度自动计算分块阈值
  ├── 短文档 → Unified 单次编译（1 次调用）
  └── 长文档 → Map-Reduce 语义分块编译
  ↓
[流水线执行] → 实体提取 + 卡片生成
  ↓
[质量门控] → reject_reason 非空或 status≠Accepted 的卡片不输出
  ↓
[输出] → Markdown 卡片 + 实体列表 + 质量报告
  ↓
[记录持久化] → SQLite 编译历史
```

### 分块策略

分块阈值根据模型**上下文窗口 + 最大输出 tokens** 双约束动态计算：

```
input_based = (context_length - prompt - safety) / 1.3
output_based = (max_output_tokens / 700) × 4000
chunk_size = min(input_based, output_based).clamp(10K, 500K)
```

| 模型上下文 | 最大输出 | 分块阈值 | 适用场景 |
|-----------|---------|---------|---------|
| 1M / 1M tokens | ~50万字符 | 大部头书籍（如《人生模式》42万字 → 1 块） |
| 200K / 8K tokens | ~8万字符 | 中等长度文档 |
| 128K / 16K tokens | ~5万字符 | 标准长文 |
| 64K / 4K tokens | ~2万字符 | 短文档/轻量模型 |

### 卡片数量规划

卡片数量**根据文档长度动态调整**，单块上限 40 张（质量优先）：

| 文档长度 | 全书目标 | 分块数 | 每块上限 |
|---------|---------|-------|---------|
| ≤2万字 | 12-22 张 | 1 块 | 22 张 |
| 2-6万字 | 25-45 张 | 1 块 | 40 张 |
| 6-15万字 | 45-80 张 | 1-2 块 | 40 张 |
| 15-35万字 | 80-140 张 | 1-3 块 | 40 张 |
| 35-70万字 | 120-200 张 | 2-5 块 | 40 张 |

示例：《人生模式》42万字 → 预计 120-140 张卡片（分 2-3 块编译）。

## 项目结构

```
src/
├── main.rs          # CLI 入口（clap）
├── lib.rs           # 库模块声明
├── pipeline.rs      # 编译流水线 + 动态分块 + 重试逻辑
├── api.rs           # LLM 客户端 + JSON 降级 + 用量统计
├── converter.rs     # 文档 → Markdown（多层 fallback）
├── models.rs        # 数据模型（Card / Entity）
├── config.rs        # 配置管理（Provider / 书籍列表 / 限流 / 动态分块）
├── providers.rs     # LLM 提供商注册表
├── health.rs        # Provider 健康检测与自动选择
├── rate_limiter.rs  # Token-bucket 限流器
├── scan.rs          # PDF 预扫描
├── dedup.rs         # 语义去重（自适应 Jaccard + LCS）
├── batch.rs         # SQLite 编译记录持久化
├── diagnostics.rs   # 环境诊断
├── error.rs         # 错误类型
├── doc_type.rs      # 文档类型检测
├── output.rs        # 结果输出 + 质量报告
├── stages/
│   ├── entities.rs  # 实体去重与统一
│   └── common.rs    # 阶段共享逻辑
└── quality/
    ├── card_lint.rs     # 卡片 Lint（ref 格式 / 自动修复）
    ├── typo_lint.rs     # 拼写检查
    ├── metrics.rs       # 质量评分
    ├── report.rs        # 质量报告
    └── preprocess.rs    # 输入预处理
prompts/             # Prompt 模板（全部可编辑，运行时加载）
documents/           # 原始文档自动归档（命名：{日期}_{文档名}）
output/              # 默认输出目录
```

### 输出目录结构

```
output/{日期}_{书名}/
├── all_cards.md              # 全部卡片合并（按类型分组）
├── cards/
│   ├── 术语卡.md             # 同类型卡片归集（无重复类型标题）
│   ├── 新知卡.md
│   ├── 反常识卡.md
│   ├── 金句卡.md
│   ├── 人物卡.md
│   ├── 事件卡.md
│   ├── 行动卡.md
│   ├── 图示卡.md
│   ├── 新词卡.md
│   ├── 综述卡.md
│   ├── 基础卡.md
│   └── 索引卡.md
├── entities.md               # 实体列表
├── card_quality_report.md    # 卡片质量报告
├── input_quality_report.md   # 输入质量报告
├── compile_diagnostics.md    # 编译诊断（如有失败）
└── chunks/                   # 每块原始数据（调试）
    ├── chunk_00_raw.json
    ├── chunk_00.json
    ├── chunk_00_cards.md
    └── chunk_00_entities.md
```

## 常见问题

**编译报错 Python 依赖未找到**：`uv pip install pymupdf markitdown`

**API Key 无效**：`cardc doctor` 诊断 → `cardc init` 重新配置

**扫描版 PDF 超时**：`cardc scan ./` 检测 → OCR 处理后重试

**输出质量差**：检查 `card_quality_report.md`，ref 格式/例子真实性/类型混淆均有自动检测

**编译结果为空（0 张卡片）**：
- 检查 API Key 余额与连通性：`cardc doctor`
- 检查输入质量：`cardc quality ./文件.pdf`
- 查看诊断文件：`output/.../compile_diagnostics.md`

**卡片数量太少**：
- 检查模型上下文长度：`cardc doctor` 会显示当前模型信息
- 小上下文模型（64K/128K）会自动分块，每块独立生成卡片
- 使用大上下文模型（1M tokens）可避免分块，一次性处理整本书

