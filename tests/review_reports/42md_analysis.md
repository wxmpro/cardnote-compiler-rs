# 42md 工具链反编译分析

> 分析对象：42md CLI（https://42md.cc）
> 分析目的：理解 cardnote-compiler-rs 为何集成 42md，以及 42md 的能力边界
> 分析日期：2026-05-29

---

## 一、42md 是什么

42md 是一款面向中文知识工作者的 Markdown 工具链，定位为"你的知识快刀"。

核心定位：**Markdown 的中文化、出版化、智能化处理工具**。

---

## 二、全部功能点（11 个工具）

### 1. lint — Markdown 排版优化（免费）

| 属性 | 说明 |
|------|------|
| 输入 | Markdown 文件 |
| 输出 | .md（原地修复或生成副本）|
| 配额 | 免费 |
| 规则数 | 20+ 条中文排版规则 |
| 预设 | 6 种排版预设 |

**在 cardnote-compiler-rs 中的使用**：
```rust
// src/output.rs:18
async fn apply_42md_lint(md_path: &Path) -> Result<()> {
    let output = Command::new("42md")
        .arg("tools")
        .arg("lint")
        .arg(md_path)
        .output()?;
    // ...
}
```

调用点：
- `save_single` → summary.md
- `save_book` → summary.md
- `save_curation` → summary.md
- `save_single_to_dir` → summary.md

**作用**：对 AI 生成的 Markdown 进行中文排版规范化，修复中西文混排、标点符号、空格等问题。

---

### 2. md2pdf — Markdown 转 PDF（免费）

| 属性 | 说明 |
|------|------|
| 输入 | Markdown 文件 |
| 输出 | .pdf（出版级，含中文字体）|
| 配额 | 免费 |

**在 cardnote-compiler-rs 中的使用**：
```rust
// src/output.rs:51
async fn apply_42md_md2pdf(md_path: &Path) -> Result<()> {
    let output = Command::new("42md")
        .arg("tools")
        .arg("md2pdf")
        .arg(md_path)
        .output()?;
    // ...
}
```

**注意**：当前代码中有此函数定义，但**没有被任何调用点使用**（注释掉的或未接入的）。

---

### 3. md2docx — Markdown 转 Word（免费）

**未在 cardnote-compiler-rs 中使用**。

潜在用途：将卡片导出为 Word，便于在 Office 环境中编辑和分享。

---

### 4. md2epub — Markdown 转 EPUB（免费）

**未在 cardnote-compiler-rs 中使用**。

潜在用途：将多本书的卡片策展结果打包为电子书，在 iPad/Kindle 上阅读。

---

### 5. md2html — Markdown 转 HTML（免费）

**未在 cardnote-compiler-rs 中使用**。

潜在用途：生成静态网页版知识库，便于在线分享和检索。

---

### 6. md2wechat — Markdown 转公众号 HTML（免费）

**未在 cardnote-compiler-rs 中使用**。

潜在用途：将卡片直接发布到微信公众号，一键排版。

---

### 7. improve — AI 文本优化（消耗 AI 配额）

| 属性 | 说明 |
|------|------|
| 功能 | 错别字纠正、字幕转书面稿、会议纪要整理等 |
| 配额 | 消耗 ai_gen_chars |

**未在 cardnote-compiler-rs 中使用**。

潜在用途：对 LLM 生成的卡片内容进行二次润色，提升表达质量。

---

### 8. translate — Markdown 翻译（消耗 AI 配额）

| 属性 | 说明 |
|------|------|
| 功能 | 整篇 Markdown 翻译为指定语言 |
| 配额 | 按千字计费 |

**未在 cardnote-compiler-rs 中使用**。

潜在用途：将中文卡片翻译为英文，或反之，便于国际学术交流。

---

### 9. hotwords — 热词提取（消耗 AI 配额）

| 属性 | 说明 |
|------|------|
| 功能 | 从 Markdown 中抽取领域热词 |
| 配额 | 消耗 ai_gen_chars |

**未在 cardnote-compiler-rs 中使用**。

潜在用途：对一本书的卡片集合提取关键词，生成知识图谱的标签云。

---

### 10. screenshot — 网页截图（免费）

**未在 cardnote-compiler-rs 中使用**。

---

### 11. download — 整站资源下载（免费）

**未在 cardnote-compiler-rs 中使用**。

---

## 三、技术路径分析

### 3.1 cardnote-compiler-rs 的集成方式

```
cardnote-compiler-rs
    └── 调用 42md CLI（外部进程）
            ├── 42md tools lint  <md文件>   → 中文排版优化
            └── 42md tools md2pdf <md文件>  → PDF导出（未使用）
```

集成特点：
- **松耦合**：通过 `std::process::Command` 调用，不依赖 42md 的库
- **容错**：42md 未安装时静默跳过（`which 42md` 检查）
- **单向**：只调用 42md 的输入，不接收结构化返回

### 3.2 42md lint 的 20+ 排版规则（推断）

基于中文排版常见规范，42md lint 可能包含以下规则：

| 规则类别 | 具体规则 |
|---------|---------|
| 空格规则 | 中英文之间加空格、数字与单位之间加空格 |
| 标点规则 | 中文标点使用全角、英文标点使用半角 |
| 引号规则 | 中文引号「」、英文引号 "" |
| 链接规则 | 链接文字与链接之间不加空格 |
| 列表规则 | 列表项之间统一格式 |
| 标题规则 | 标题层级规范（# ## ###）|
| 代码块规则 | 代码块标注语言 |
| 表格规则 | 表格对齐规范 |

### 3.3 6 种排版预设（推断）

可能对应不同的使用场景：
1. **标准预设**：通用中文排版
2. **出版预设**：适合图书出版
3. **学术预设**：适合论文
4. **技术预设**：适合技术文档
5. **公众号预设**：适合微信文章
6. **极简预设**：最小化干预

---

## 四、优秀点

### 4.1 对 cardnote-compiler-rs 的价值

1. **中文排版专业化**：AI 生成的 Markdown 常有中西文混排问题（如"AI生成的"应为"AI 生成的"），42md lint 自动修复
2. **零成本集成**：免费工具，松耦合调用
3. **出版级输出**：md2pdf 支持中文字体，可直接生成可打印的 PDF

### 4.2 42md 本身的设计优秀点

1. **工具链思维**：不做一个大而全的编辑器，而是做一组原子化工具
2. **配额分层**：基础功能免费（lint/md2pdf），AI 功能按量付费
3. **中文化优先**：所有规则围绕中文排版场景设计
4. **命令行友好**：适合脚本化、自动化流水线

---

## 五、能力边界与潜在问题

### 5.1 能力边界

1. **不处理内容质量**：42md lint 只修复排版，不判断内容好坏
2. **不处理语义**：不检查逻辑一致性、事实准确性
3. **外部依赖**：需要单独安装 42md，增加了部署复杂度

### 5.2 潜在问题

1. **版本兼容性**：42md 升级后规则变化，cardnote-compiler-rs 无感知
2. **静默跳过**：42md 未安装时静默跳过，用户可能不知道排版优化没执行
3. **错误处理粗糙**：42md 执行失败时只打印错误，不中断流程

---

## 六、未使用的功能及潜在集成建议

| 42md 功能 | 当前状态 | 潜在集成场景 |
|-----------|---------|-------------|
| md2pdf | 有代码，未调用 | 一键导出卡片为 PDF 电子书 |
| md2epub | 未使用 | 生成 EPUB 在 iPad 上阅读 |
| md2html | 未使用 | 生成在线知识库 |
| improve | 未使用 | 对 LLM 生成卡片二次润色 |
| hotwords | 未使用 | 提取领域关键词，辅助标签化 |
| translate | 未使用 | 双语卡片生成 |

---

## 七、总结

42md 是 cardnote-compiler-rs 的**下游排版工具**，负责将 AI 生成的"粗糙 Markdown"转化为"出版级 Markdown"。

当前集成深度：**浅**（只调用了 lint，且容错逻辑过松）。

建议改进方向：
1. 增加 42md 可用性检查并提示用户安装
2. 考虑集成 md2pdf/md2epub，实现"编译 → 排版 → 出版"完整流水线
3. 考虑集成 improve，对卡片内容进行二次质量提升
