# CardNote Compiler (Rust 版)

把任意文档（PDF、Word、Markdown、EPUB 等）编译成结构化的知识卡片。

**当前版本**：v0.1.24

**核心功能**：
- 自动检测扫描版 PDF → 提示 OCR 处理
- AI 提取摘要、实体标注、知识卡片（12 种类型）、关系图谱
- 支持单篇编译（≤50K 字符）和 Map-Reduce 分块编译（长文档）
- 14+ LLM 提供商自动健康检测与智能选择
- 编译缓存（断点续编译）+ Stage 级缓存
- 输入质量检测与质量门控

---

## 目录

1. [环境要求](#1-环境要求)
2. [安装](#2-安装)
3. [首次配置](#3-首次配置)
4. [完整使用流程](#4-完整使用流程)
5. [命令详解](#5-命令详解)
6. [扫描版 PDF 处理](#6-扫描版-pdf-处理)
7. [配置说明](#7-配置说明)
8. [架构与实现](#8-架构与实现)
9. [常见问题](#9-常见问题)

---

## 1. 环境要求

| 组件 | 版本 | 用途 | 是否必须 |
|------|------|------|---------|
| **macOS** | 14+ | 运行环境 | 是 |
| **Rust** | 1.85+ | 编译本工具 | 是 |
| **Python** | 3.11+ | PDF 解析 fallback | 推荐 |
| **uv** | 最新 | Python 包管理 | 推荐 |
| **markitdown** | 最新 | 通用文档转换 | 推荐 |
| **pymupdf** | 最新 | PDF 文本提取 | 推荐 |
| **MinerU** | 最新 | 扫描版 PDF OCR | 扫描版 PDF 时需要 |
| **PDF Expert** | 3.x | macOS 批量 OCR（质量最佳） | 扫描版 PDF 时推荐 |

### 1.1 检查 Rust 环境

```bash
rustc --version    # 应输出 1.85.0 或更高
cargo --version    # 应输出 1.85.0 或更高
```

### 1.2 安装 Python 依赖

```bash
cd ./cardnote-compiler-rs

# 使用 uv 安装（推荐）
uv pip install pymupdf markitdown

# 如果需要处理扫描版 PDF，额外安装 MinerU
uv pip install -U 'mineru[core]'
```

---

## 2. 安装

### 2.1 编译

```bash
cargo build --release

# 二进制文件在 target/release/cardc
# 建议创建软链接方便全局调用
ln -s $(pwd)/target/release/cardc ~/.local/bin/cardc
```

### 2.2 验证安装

```bash
cardc --version
# 应输出: cardc 0.1.24
```

---

## 3. 首次配置

### 3.1 交互式配置向导

```bash
cardc init
```

向导会：
1. 自动扫描环境中已配置的 AI 提供商（环境变量、配置文件）
2. 引导添加未配置的提供商
3. 选择默认使用的提供商

支持自动扫描的提供商配置来源：
- 环境变量（`LLM_API_KEY`、`DEEPSEEK_API_KEY`、`ANTHROPIC_API_KEY` 等）
- Claude Code 配置（`~/.claude/settings.json`）
- PDFMathTranslate 配置
- 自定义 JSON 配置文件（`~/.config/cardnote/providers.json`）

### 3.2 配置文件位置

配置默认保存到用户目录，避免把 API Key 写进项目仓库：

```
~/.config/cardnote/.env          # 主配置
~/.config/cardnote/providers.json  # 用户自定义 Provider 覆盖
```

`.env` 格式：

```
LLM_API_KEY=sk-your-key-here
LLM_PROVIDER=deepseek
LLM_MODEL=deepseek-chat
```

### 3.3 外部 Provider 配置（无需重编译）

在 `~/.config/cardnote/providers.json` 中定义自定义 Provider，重启工具即可生效：

```json
[
  {
    "id": "my-provider",
    "name": "My Provider",
    "aliases": ["my"],
    "default_base_url": "https://api.example.com/v1",
    "api_key_env_var": "MY_API_KEY",
    "api_key_pattern": "sk-",
    "protocol": "open_ai_compatible",
    "models": [
      {
        "id": "model-1",
        "name": "Model 1",
        "context_length": 128000,
        "max_output_tokens": 8192,
        "supports_json_mode": true,
        "supports_vision": false,
        "description": "描述"
      }
    ]
  }
]
```

---

## 4. 完整使用流程

### 4.1 标准流程（已有文字层的 PDF）

```bash
# 直接编译
cardc ./my-document.pdf

# 查看输出
ls ./output/
#  └── 20260531_my-document/
#       ├── summary.md              # AI 摘要
#       ├── all_cards.md            # 全部卡片汇总
#       ├── cards/                  # 按类型分类的卡片
#       ├── graph.mmd               # 关系图谱（Mermaid 格式）
#       ├── entities.md             # 实体列表
#       ├── input_quality_report.md # 输入质量报告
#       └── card_quality_report.md  # 卡片质量报告
```

### 4.2 扫描版 PDF 流程

```bash
# Step 1: 扫描检测
cardc scan ~/my-pdf-folder --recursive

# Step 2: 按提示 OCR 处理（macOS + PDF Expert 推荐）
# Step 3: 编译 OCR 后的文件
cardc ~/my-pdf-folder/document_OCR.pdf
```

### 4.3 长文档分块编译

超过 50,000 字符的文档自动触发 Map-Reduce 模式：

```bash
cardc ./长篇小说.pdf
# [模式] 分块 Map-Reduce 编译
# [1/4] 语义分块...
# [2/4] Map 阶段 — 串行编译...（支持断点续编译）
# [3/4] Reduce 阶段 — 合并去重...
# [4/4] 生成全局摘要...
```

**特性**：
- 自动语义分块（按 Markdown 标题层级）
- 块间保留 2000 字符重叠上下文
- 支持断点续编译（崩溃后重新运行自动跳过已完成块）
- 失败率 > 20% 才中断，否则继续处理成功块

---

## 5. 命令详解

### 5.1 `cardc <文件>` — 完整编译（默认命令）

```bash
# 基本用法
cardc ./document.pdf

# 指定输出目录
cardc ./document.pdf --output ./my_output

# 强制继续（低质量输入不中止）
cardc ./document.pdf --force

# 指定提供商和模型
cardc ./document.pdf --provider openai --model gpt-4o

# 指定 API Key（不推荐，会留在 shell history）
cardc ./document.pdf --api-key sk-xxx --base-url https://api.example.com
```

**输出文件**：
- `summary.md` — 文档摘要（标题、概述、核心要点、结构）
- `all_cards.md` — 全部卡片汇总
- `cards/` — 按类型分文件的知识卡片
  - `新知卡.md` — 新观点/新知识
  - `术语卡.md` — 专业术语定义
  - `人物卡.md` — 关键人物
  - `事件卡.md` — 重要事件
  - `行动卡.md` — 可执行的方法/步骤
  - `金句卡.md` — 精彩引用（含原文/出处/仿写）
  - `图示卡.md` — 图表/可视化概念
  - `新词卡.md` — 新兴词汇
  - `基础卡.md` — 基础概念笔记
  - `索引卡.md` — 快速检索入口
  - `反常识卡.md` — 反直觉发现
  - `综述卡.md` — 跨章节综合
- `graph.mmd` — 实体关系图谱（Mermaid 语法）
- `entities.md` — 实体列表
- `input_quality_report.md` — 输入质量检测报告
- `card_quality_report.md` — 卡片质量检测报告

**编译前自动执行**：
1. Provider 健康检测（自动选择延迟最低的可用提供商）
2. PDF 文字层探测（扫描版自动提示）
3. 输入质量检测（质量过低默认中止，可用 `--force` 覆盖）

### 5.2 `cardc scan` — PDF 预扫描

```bash
# 扫描当前目录
cardc scan .

# 递归扫描
cardc scan ~/Documents/PDFs --recursive

# 自定义阈值（默认 20 字符）
cardc scan . --threshold 50
```

**输出**：分类统计（已有文字层 / 需 OCR / 手绘版 / 异常）+ 下一步建议

### 5.3 `cardc quality` — 质量检测

```bash
cardc quality ./document.pdf
```

**检测维度**：字符健康度、结构完整性、噪声污染度、语义连贯性、内容完整性

**输出**：综合评分（A/B/C/D/F）+ 关键问题列表

### 5.4 `cardc summary` — 仅生成摘要

```bash
cardc summary ./document.pdf --output ./output
```

### 5.5 `cardc annotate` — 仅实体标注

```bash
cardc annotate ./document.pdf
```

输出实体列表到控制台（不保存文件）。

### 5.6 `cardc cards` — 仅生成知识卡片

```bash
cardc cards ./document.pdf --output ./output
```

### 5.7 `cardc graph` — 仅生成关系图谱

```bash
cardc graph ./document.pdf
```

输出 Mermaid 格式图谱到控制台。

### 5.8 `cardc doctor` — 环境诊断

```bash
cardc doctor
```

检查：API 配置、Python 依赖、PDF 工具、网络连通性。

### 5.9 `cardc init` — 重新配置

```bash
cardc init
```

重新运行交互式配置向导，支持多提供商配置。

---

## 6. 扫描版 PDF 处理

### 6.1 判断标准

1. **lopdf 纯 Rust 检测**（默认）：检查 Resources → Font 字典
   - 有字体定义 → 已有文字层
   - 无字体 + 有图片 → 扫描版（需 OCR）
2. **PyMuPDF fallback**：lopdf 解析失败时
3. **可读字符比例 < 阈值** → 扫描版

### 6.2 处理优先级（质量由高到低）

| 方案 | 工具 | 适用平台 | 质量 |
|------|------|----------|------|
| 1 | PDF Expert 批量 OCR | macOS | ⭐⭐⭐ |
| 2 | MinerU | 全平台 | ⭐⭐⭐ |
| 3 | pytesseract + pdf2image | 全平台 | ⭐⭐ |

### 6.3 macOS 用户（PDF Expert，推荐）

```bash
# 设置环境变量（可选，自动探测标准路径）
export CARDNOTE_OCR_PROJECT_PATH=~/projects/pdf-expert-batch-ocr

# cardc 会自动调用 pdf-expert-batch-ocr
```

### 6.4 非 macOS 用户

```bash
uv pip install -U 'mineru[core]'
# cardc 会自动调用 MinerU
```

---

## 7. 配置说明

### 7.1 阶段级模型配置（Tiered Strategy）

不同编译阶段可指定不同模型：

```bash
export LLM_MODEL_SUMMARY=gpt-4o      # 摘要阶段
export LLM_MODEL_ENTITIES=gpt-4o     # 实体提取
export LLM_MODEL_CARDS=deepseek-chat # 卡片生成
export LLM_MODEL_GRAPH=deepseek-chat # 图谱构建
```

### 7.2 动态 max_tokens

根据文档长度自动计算各阶段 token 上限（避免 JSON 截断）：

| 阶段 | 策略 |
|------|------|
| summary | 固定 2000 |
| entities | min(max(文档长度/20, 4000), 8000) |
| graph | 固定 10000 |
| cards | min(max(文档长度/8, 3000), 8000) |

### 7.3 缓存控制

```bash
# 禁用 Stage 级缓存
export CARDC_DISABLE_STAGE_CACHE=1

# 缓存目录
./.cardc_cache/          # 分块编译缓存
./.cardc_cache/stages/   # Stage 级缓存
```

### 7.4 Prompt 目录

```bash
# 自定义 Prompt 目录
export CARDNOTE_PROMPTS_DIR=/path/to/prompts
```

默认搜索顺序：环境变量 → 可执行文件旁 → 项目根目录 `prompts/`。

---

## 8. 架构与实现

### 8.1 项目结构

```
cardnote-compiler-rs/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI 入口
│   ├── lib.rs               # 模块导出
│   ├── pipeline.rs          # 编译流水线（单轮 + Map-Reduce）
│   ├── api.rs               # LLM 客户端（多协议 + 重试 + 用量追踪）
│   ├── converter.rs         # 文档 → Markdown 转换（多层 fallback）
│   ├── scan.rs              # PDF 预扫描（lopdf + PyMuPDF）
│   ├── quality/             # 质量系统
│   │   ├── preprocess.rs    # 文本预处理
│   │   ├── card_lint.rs     # 卡片质量过滤
│   │   ├── typo_lint.rs     # 中文排版修复
│   │   └── pdf_postprocess.rs # PDF 后处理
│   ├── stages/              # 编译阶段
│   │   ├── summary.rs       # AI 摘要
│   │   ├── entities.rs      # 实体提取
│   │   ├── cards.rs         # 卡片生成（12 种类型）
│   │   └── graph.rs         # 关系图谱
│   ├── models.rs            # 数据模型
│   ├── output.rs            # 结果输出
│   ├── config.rs            # 配置管理
│   ├── providers.rs         # 提供商注册表（14+ 内置）
│   ├── health.rs            # 健康检测
│   ├── diagnostics.rs       # 诊断报告
│   ├── dedup.rs             # 去重合并
│   ├── doc_type.rs          # 文档类型检测
│   └── error.rs             # 错误类型
├── prompts/                 # Prompt 模板（可自定义）
│   ├── summary.md
│   ├── entity_extraction.md
│   ├── all_cards.md
│   ├── relation_graph.md
│   └── ...（12 种卡片类型）
├── tests/
│   ├── review_reports/      # 评估报告
│   └── ...
└── README.md
```

### 8.2 编译流水线

```
输入文档
  ↓
[PDF探测] → 扫描版？→ OCR处理
  ↓
[质量检测] → 质量过低？→ 中止（或 --force）
  ↓
[文档转换] → Markdown
  ↓
[分块判断] ≤50K字符？→ 单轮编译 : Map-Reduce分块
  ↓
[摘要] + [实体]（并行）→ [卡片] → [图谱]
  ↓
[去重合并] + [质量过滤] + [排版修复]
  ↓
输出结果（Markdown + Mermaid）
```

### 8.3 支持的 LLM 提供商

内置 14 家提供商，支持外部 JSON 配置扩展：

| 提供商 | 协议 | JSON Mode |
|--------|------|-----------|
| OpenAI | OpenAI-compatible | ✅ |
| DeepSeek | OpenAI-compatible | ✅ |
| Anthropic Claude | OpenAI-compatible | ✅ |
| Google Gemini | OpenAI-compatible | ✅ |
| Moonshot Kimi | OpenAI-compatible | ✅ |
| 阿里云通义千问 | OpenAI-compatible | ✅ |
| 智谱 GLM | OpenAI-compatible | ✅ |
| 字节豆包 | OpenAI-compatible | ✅ |
| ZSCC | OpenAI-compatible | 部分 |
| NVIDIA NIM | OpenAI-compatible | ✅ |
| Linux.do Hub | OpenAI-compatible | ✅ |
| Hugging Face | OpenAI-compatible | ✅ |
| LPGPT | OpenAI-compatible | ✅ |
| AI1216 | OpenAI-compatible | ✅ |
| DGBMC | OpenAI-compatible | ✅ |

---

## 9. 常见问题

### Q1: `cardc` 命令找不到

```bash
# 使用完整路径
./target/release/cardc

# 或临时使用 cargo run
cargo run --release -- ./document.pdf
```

### Q2: 编译报错 "Python 依赖未找到"

```bash
uv pip install pymupdf markitdown
```

### Q3: 扫描版 PDF 处理超时

```bash
# 先 OCR 再编译
cardc scan ./
# 按提示 OCR 处理后，再运行 cardc
```

### Q4: API Key 无效

```bash
cardc doctor        # 诊断环境
cardc init          # 重新配置
```

### Q5: 某个分块编译失败

工具已内置多层容错：
- **API 层**：自动重试 3 次，指数退避（2s → 4s → 8s）
- **JSON 层**：解析失败自动切换 fallback 模型
- **Stage 层**：每个阶段独立重试
- **分块层**：失败块整体重试一次，失败率 > 20% 才中断

失败信息会写入 `compile_diagnostics.md`。

### Q6: 编译结果为空（摘要和卡片都是空的）

可能原因：
1. LLM 输出全部为空 — 检查 API 连通性（`cardc doctor`）
2. 卡片解析未匹配到有效内容 — 检查 `compile_diagnostics.md`
3. 输入文档本身内容极少 — 检查 `input_quality_report.md`

### Q7: 输出质量不好（乱码、重复内容）

```bash
cardc quality ./document.pdf
```

根据报告修复：乱码多 → 换 OCR 工具；重复多 → 原文档有水印/页眉页脚。

---

## 发布与隐私边界

本仓库只应发布源码、测试、脱敏示例和说明文档。以下内容必须留在本机：

- API Key、`.env`、`.cardnote/` provider 配置
- 用户真实 PDF、Word、PPT 等原始输入
- `output/`、`.cardc_cache/`、`target/`、临时转换产物
- 生成后的真实卡片、摘要、实体、图谱
- `.DS_Store`、IDE 配置

发布前必须执行：

```bash
git status --short
git diff --stat
```

确认变更集中不包含真实输入、真实产出、缓存和密钥后，方可提交。
