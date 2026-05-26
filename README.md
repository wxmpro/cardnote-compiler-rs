# CardNote Compiler (Rust 版)

把任意文档（PDF、Word、Markdown 等）编译成结构化的知识卡片。

**核心功能**：
- 自动检测扫描版 PDF → 提示 OCR 处理
- AI 提取摘要、实体标注、知识卡片、关系图谱
- 支持单篇编译和多文档策展模式

---

## 目录

1. [环境要求](#1-环境要求)
2. [安装](#2-安装)
3. [首次配置](#3-首次配置)
4. [完整使用流程](#4-完整使用流程)
5. [命令详解](#5-命令详解)
6. [扫描版 PDF 处理](#6-扫描版-pdf-处理)
7. [配置说明](#7-配置说明)
   -- [相关项目](#相关项目)
   -- [发布与隐私边界](#发布与隐私边界)

---

## 1. 环境要求

| 组件 | 版本 | 用途 | 是否必须 |
|------|------|------|---------|
| **macOS** | 14+ | 运行环境 | 是 |
| **Rust** | 1.85+ | 编译本工具 | 是 |
| **Python** | 3.11+ | PDF 解析 fallback | 推荐 |
| **uv** | 最新 | Python 包管理 | 推荐 |
| **markitdown** | 最新 | 通用文档转换 | 推荐 |
| **MinerU** | 最新 | 扫描版 PDF OCR | 扫描版 PDF 时需要 |
| **PDF Expert** | 3.x | macOS 批量 OCR | 扫描版 PDF 时推荐 |

### 1.1 检查 Rust 环境

```bash
rustc --version    # 应输出 1.85.0 或更高
```

如果没有 Rust，通过 [rustup](https://rustup.rs/) 安装：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### 1.2 检查 Python 环境

```bash
python3 --version   # 应输出 3.11.x 或更高
```

### 1.3 安装 Python 依赖

```bash
# 进入项目目录
cd /Users/xinmin/openmind/03_Own_project/01-cardnote/03-项目/cardnote-compiler-rs

# 使用 uv 安装（推荐）
uv pip install pymupdf markitdown

# 如果需要处理扫描版 PDF，额外安装 MinerU
uv pip install -U 'mineru[core]'
```

---

## 2. 安装

### 2.1 编译

```bash
# 在项目根目录执行
cargo build --release

# 编译完成后，二进制文件在 target/release/cardc
# 可以创建软链接方便全局调用
ln -s $(pwd)/target/release/cardc ~/.local/bin/cardc
```

### 2.2 验证安装

```bash
cardc --version
# 应输出: cardc 0.1.0

cardc --help
# 应列出所有子命令
```

---

## 3. 首次配置

首次使用前需要配置 LLM API。

```bash
cardc init
```

按提示输入：
1. **API Key**：你的 OpenAI / DeepSeek / 其他服务商的 API Key
2. **服务商**：选择 `openai`、`deepseek` 或自定义
3. **模型名称**：如 `gpt-4o`、`deepseek-chat`

配置默认保存到用户配置目录，避免把 API Key 写进项目仓库：

```
~/.config/cardnote/.env
```

格式如下：

```
LLM_API_KEY=sk-your-key-here
LLM_PROVIDER=deepseek
LLM_MODEL=deepseek-chat
LLM_BASE_URL=https://api.deepseek.com/v1
```

---

## 4. 完整使用流程

### 4.1 标准流程（已有文字层的 PDF）

```bash
# Step 1: 直接编译
cardc ./我的文档.pdf

# Step 2: 查看输出
ls ./output/
#  └── 20260523_人生模式：识别并优化你的核心认知/
#       ├── summary.md    # AI 摘要
#       ├── all_cards.md  # 全部卡片汇总
#       ├── graph.mmd     # 关系图谱（Mermaid 格式）
#       ├── entities.md   # 实体列表
#       └── cards/        # 按类型分类的卡片
```

### 4.2 扫描版 PDF 流程

```bash
# Step 1: 先扫描，检测哪些需要 OCR
cardc scan ~/我的PDF文件夹

# Step 2: 如果有"需 OCR"的文件，按提示运行 OCR
# （macOS + PDF Expert 用户）
cd /Users/xinmin/openmind/03_Own_project/11-pdf-expert-batch-ocr
python scan_pdfs.py ~/我的PDF文件夹 --output ocr_queue.json
python batch_ocr.py --queue ocr_queue.json --output-dir ./output

# Step 3: OCR 完成后，编译 *_OCR.pdf 文件
cardc ~/我的PDF文件夹/某文档_OCR.pdf
```

### 4.3 多文档策展

把多篇文档合并成一个主题知识库：

```bash
# 准备一个文件夹，里面放多个 .md 文件
mkdir ./source_docs
cp doc1.md doc2.md doc3.md ./source_docs/

# 编译
cardc ./source_docs --book "AI 学习笔记"

# 输出一个完整的策展文档
```

---

## 5. 命令详解

### 5.1 `cardc` — 完整编译（默认命令）

```bash
# 基本用法
cardc <文件路径>

# 指定输出目录
cardc ./document.pdf -o ./my_output

# 指定模型（覆盖默认配置）
cardc ./document.pdf --provider openai --model gpt-4o

# 多文档策展
cardc ./docs_folder --book "读书笔记"
```

**输出**：
- `summary.md` — 文档摘要（标题、要点、结论）
- `all_cards.md` — 全部卡片汇总
- `cards/` — 知识卡片，按类型分文件
  - `术语卡.md` — 专业术语定义
  - `新知卡.md` — 新观点/新知识
  - `行动卡.md` — 可执行的方法/步骤
  - `金句卡.md` — 精彩引用
  - `人物卡.md` — 关键人物
- `graph.mmd` — 实体关系图谱（Mermaid 语法）
- `entities.md` — 实体列表

### 5.2 `cardc scan` — PDF 预扫描

检测目录下的 PDF 哪些需要 OCR，哪些可以直接编译。

**技术特点**：
- 纯 Rust 实现（lopdf），无需启动 Python 子进程
- 异步并行检测（tokio），多文件同时处理
- PyMuPDF fallback，对损坏/非标准 PDF 更宽容

```bash
# 扫描当前目录
cardc scan .

# 递归扫描
cardc scan ~/Documents/PDFs -r

# 自定义阈值（默认 20 字符）
cardc scan . -t 50
```

**输出示例**：

```
共找到 10 个 PDF 文件，开始检测文字层...
[  1/10] 已有文字层  | book1.pdf              | 已提取 3200 个字符
[  2/10] 需 OCR      | scanned_doc.pdf        | 仅 3 字符，判定为扫描版
...

分类统计
   ✓ 已有文字层: 8 (80.0%)
   ⚠ 需 OCR:     2 (20.0%)

下一步建议
   检测到 macOS + OCR 项目可用，建议批量 OCR:
     ❯ cd /Users/xinmin/.../11-pdf-expert-batch-ocr
     ❯ python scan_pdfs.py . --output ocr_queue.json
     ❯ python batch_ocr.py --queue ocr_queue.json --output-dir ./output

   以下文件可直接编译:
     ✓ book1.pdf
     ✓ paper2.pdf
     ...
```

### 5.3 `cardc quality` — 质量检测

检测 PDF 解析质量，发现潜在问题。

```bash
cardc quality ./document.pdf
```

**检测维度**：
1. **字符健康度** — 乱码比例、替换字符数量
2. **结构完整性** — 标题层级、段落分布
3. **噪声污染度** — 重复内容、水印
4. **语义连贯性** — 强制换行、URL 断裂
5. **内容完整性** — 空白区域、内容密度

**输出**：综合评分 + 等级（A/B/C/D/F）+ 关键问题列表

### 5.4 `cardc summary` — 仅生成摘要

```bash
cardc summary ./document.pdf -o ./output
```

输出 `summary.md`，包含：标题、核心观点、关键结论。

### 5.5 `cardc cards` — 仅生成知识卡片

```bash
cardc cards ./document.pdf -o ./output
```

输出到 `output/cards/` 目录。

### 5.6 `cardc graph` — 仅生成关系图谱

```bash
cardc graph ./document.pdf
```

输出 Mermaid 格式的实体关系图到控制台。

### 5.7 `cardc doctor` — 环境诊断

```bash
cardc doctor
```

检查：
- API 配置是否正确
- Python 依赖是否安装
- PDF 解析工具是否可用
- 网络连接是否正常

### 5.8 `cardc init` — 重新配置

```bash
cardc init
```

重新设置 API Key、服务商、模型。

---

## 6. 扫描版 PDF 处理

### 6.1 判断标准

工具通过以下方式判断 PDF 是否为扫描版：

1. **lopdf 纯 Rust 检测**（默认）：检查 Resources → Font 字典
   - 有字体定义 → 已有文字层
   - 无字体 + 无图片 + 大量间接对象 → 手绘版
   - 无字体 + 有图片 → 扫描版（需 OCR）
2. **PyMuPDF fallback**：lopdf 解析失败时（损坏/非标准 PDF）
   - 提取文本字符数 < 阈值 → 扫描版
   - 大量矢量路径 + 极少图片 → 手绘版
3. **可读字符比例 < 50%** → 扫描版

### 6.2 macOS 用户（推荐）

使用 PDF Expert 批量 OCR，质量最高。

**前提**：
- macOS 14+
- 已购买 PDF Expert（设置为默认 PDF 应用）
- 系统设置 → 隐私与安全 → 辅助功能 → 终端（或你的 IDE）已授权

**步骤**：

```bash
# 1. 进入 OCR 项目目录
cd /Users/xinmin/openmind/03_Own_project/11-pdf-expert-batch-ocr

# 2. 安装 Python 依赖
uv pip install -r requirements.txt

# 3. 预扫描（检测哪些需要 OCR）
python scan_pdfs.py ~/你的PDF文件夹 --output ocr_queue.json

# 4. 批量 OCR（自动断点续传）
python batch_ocr.py \
  --queue ocr_queue.json \
  --output-dir ./output \
  --db progress.db

# 5. 等待完成，输出 *_OCR.pdf 文件
# 6. 用 cardc 编译 OCR 后的文件
cardc ./output/某文档_OCR.pdf
```

**特性**：
- 原文件始终不受影响（操作副本）
- 断点续传（Ctrl+C 安全中断，重新运行自动续传）
- 每 10 个文件自动重启 PDF Expert 释放内存
- OCR 后自动验证文字层是否真正生成

### 6.3 非 macOS 用户

使用 MinerU 进行 OCR：

```bash
# 安装
uv pip install -U 'mineru[core]'

# 使用（cardc 会自动调用）
cardc ./扫描版文档.pdf
```

### 6.4 没有 OCR 工具时的表现

```bash
cardc ./扫描版.pdf
```

输出：

```
错误: 扫描版/图片 PDF 需要 OCR 工具。
  方案1（推荐）: uv pip install -U 'mineru[core]'
  方案2: brew install tesseract && uv pip install pytesseract pdf2image

  💡 PDF '扫描版.pdf' 为扫描版/图片版，需要 OCR 处理

     macOS 用户建议:
       方案1: PDF Expert 手动 OCR（质量最好）
       方案2: 安装 MinerU: uv pip install -U 'mineru[core]'
```

---

## 7. 配置说明

### 7.1 配置文件位置

```
~/.config/cardnote/.env
```

### 7.2 配置项

| 变量 | 说明 | 示例 |
|------|------|------|
| `LLM_API_KEY` | LLM API Key | `sk-xxx...` |
| `LLM_PROVIDER` | 服务商 | `openai`, `deepseek` |
| `LLM_MODEL` | 模型名称 | `gpt-4o`, `deepseek-chat` |
| `LLM_BASE_URL` | 自定义 API 地址（可选） | `https://api.deepseek.com` |

### 7.3 命令行覆盖

所有配置都可以通过命令行参数覆盖：

```bash
cardc ./doc.pdf --provider openai --model gpt-4o --api-key sk-xxx
```

---

## 8. 常见问题

### Q1: `cardc` 命令找不到

```bash
# 方法1: 使用完整路径
/Users/xinmin/openmind/03_Own_project/01-cardnote/03-项目/cardnote-compiler-rs/target/release/cardc

# 方法2: 创建软链接
ln -s /Users/xinmin/openmind/03_Own_project/01-cardnote/03-项目/cardnote-compiler-rs/target/release/cardc ~/.local/bin/cardc

# 方法3: 临时使用 cargo run
cargo run -- ./document.pdf
```

### Q2: 编译报错 "Python 依赖未找到"

```bash
# 安装 PyMuPDF（必须）
uv pip install pymupdf

# 安装 markitdown（推荐）
uv pip install markitdown
```

### Q3: 扫描版 PDF 处理超时

```bash
# quality 命令默认 60 秒短超时
# 完整编译会根据页数自动估算超时

# 如果仍然超时，先 OCR 再编译
cardc scan ./          # 检测哪些需要 OCR
# 按提示 OCR 处理后，再运行 cardc
```

### Q4: API Key 无效

```bash
# 检查配置
cardc doctor

# 重新配置
cardc init
```

### Q5: PDF Expert OCR 窗口找不到

1. 确保 PDF Expert 是默认 PDF 应用
2. 确保 PDF Expert 界面是**中文**
3. 系统设置 → 隐私与安全 → 辅助功能 → 你的终端/IDE → 勾选
4. OCR 期间不要操作鼠标/键盘

### Q6: 某个分块编译失败（如"块 5 JSON 解析失败"）

这是 LLM 输出偶发不稳定导致的，工具已内置双层重试：

- **API 层**：`chat_json` 自动重试 3 次，指数退避（2s → 4s）
- **Stage 层**：`compile_chunk` 每个阶段（摘要/实体/卡片/图谱）自动重试 2 次

重试失败率 > 20% 才会中断整个编译，一般情况下偶发失败的块会被自动恢复。

### Q7: 输出质量不好（乱码、重复内容）

```bash
# 先检查质量
cardc quality ./document.pdf

# 根据报告修复：
# - 乱码多 → 换 OCR 工具重新处理
# - 重复多 → 原文档有水印/页眉页脚，预处理后再编译
```

---

## 项目结构

```
cardnote-compiler-rs/
├── Cargo.toml              # Rust 依赖
├── src/
│   ├── main.rs             # CLI 入口
│   ├── converter.rs        # PDF/文档 → Markdown 转换
│   ├── scan.rs             # PDF 预扫描（lopdf 纯 Rust + PyMuPDF fallback）
│   ├── quality/            # 质量检测
│   ├── pipeline.rs         # AI 处理流水线（含分块重试）
│   ├── api.rs              # LLM API 客户端（含 JSON 重试）
│   ├── output.rs           # 结果输出
│   └── ...
└── README.md               # 本文件
```

---

## 相关项目

- **PDF Expert Batch OCR**：`11-pdf-expert-batch-ocr/` — macOS 批量 OCR 流水线

---

## 发布与隐私边界

本仓库只应发布源码、测试、脱敏示例和说明文档。以下内容必须留在本机，不进入 GitHub：

- API Key、`.env`、`.env.*`、`.cardnote/` provider 配置；
- 用户真实 PDF、Word、PPT、Excel、EPUB 等原始输入；
- `output/`、`.cardc_cache/`、`target/`、临时转换产物；
- 生成后的真实卡片、摘要、实体、图谱和质量报告；
- `.DS_Store`、IDE 配置、Claude 本地会话配置。

发布前必须执行：

```bash
git status --short
git diff --stat
```

只有确认变更集中不包含真实输入、真实产出、缓存和密钥后，才可以准备提交。创建远端仓库、设置公开/私有、保留历史、推送代码或创建 release 都需要单独确认。
