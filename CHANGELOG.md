# Changelog

All notable changes to cardnote-compiler-rs are documented here.

## [0.1.9] - 2026-05-29

### Fixed

- **修复 ref 格式过滤的系统性误杀**（`card_lint.rs` `check_ref_format`）
  - **根因**：v0.1.7 引入的 ref 强制检查过于 rigid，导致 31/235（13%）卡片被误过滤。
  - **问题 1：全角分隔符 `｜`** — DeepSeek 等模型常输出全角 `｜`（U+FF5C）而非半角 `|`（U+007C），被判定为"分隔符数量 0"而过滤。
  - **问题 2：禁止词全局子串匹配** — "来源""作者简介"作为章节名完全合法（如"第六章 幸福的来源""作者简介"），但全局子串匹配将其误杀。
  - **修复 1**：在格式检查前自动将全角 `｜` 转换为半角 `|`。
  - **修复 2**：禁止词列表从 `["作者简介", "来源", "出处"]` 收紧为仅 `["出处"]`，并将检查位置移至书名号+页码+分隔符验证之后。有完整格式（书名号+页码+正确分隔符）的 ref 中，"来源""作者简介"作为章节名不再被误杀。
  - **影响**：《人生模式》347 页 PDF 编译，卡片从 202 张恢复至 233 张，误杀率从 13% 降至 0%。

### Changed
- **Cargo.toml**：版本号 `0.1.8` → `0.1.9`

## [0.1.8] - 2026-05-29

### Added

- **PDF 书籍元数据提取**（`converter.rs`）
  - 新增 `BookMetadata` 结构体（title, author, publisher, isbn, page_count）
  - 新增 `extract_pdf_metadata()` 函数，三层提取策略：
    1. PDF Info 字典（Title, Author, Subject）—— 最准确
    2. 文本正则匹配（版权页的"书名：XXX"）—— fallback
    3. 文件名 —— 最后手段
  - 新增 `extract_title_from_text()` 正则提取函数

### Fixed

- **修复输出目录命名 bug**（`main.rs`）
  - 根因：Map-Reduce 分块编译模式下调用 `save_book()` 时传入的是 `result.summary.title`（LLM 生成的第一章标题"导论 优化你的人生模式"），而不是文件名/书名。
  - 修复：编译前先调用 `extract_pdf_metadata()` 提取真正的书名，传入 `save_book()`。提取失败时 fallback 到文件名。
  - 影响：输出目录从 `20260529142827_导论 优化你的人生模式` 恢复为 `20260529142827_人生模式`

### Changed
- **Cargo.toml**：版本号 `0.1.7` → `0.1.8`

## [0.1.7] - 2026-05-29

### Added

- **ref 格式强制检查**（`card_lint.rs`）
  - 新增 `InvalidRefFormat` 致命问题，不符合规范的卡片直接过滤。
  - 检查规则：
    1. ref 不能为空
    2. 必须包含书名号 `《...》`
    3. 必须包含页码标记 `第...页`
    4. 禁止无意义内容：`作者简介`、`来源`、`出处`
    5. 分隔符 `|` 数量必须为 1 个（PDF）或 2 个（书籍）
    6. 书名号内必须有具体名称
  - 新增 7 个单元测试覆盖有效/无效格式。

### Fixed

- **prompt ref 格式区分书籍与 PDF**（`all_cards.md`）
  - 明确判断标准：文档中有章节标题→书籍格式，只有页码→PDF格式。

### Changed
- **Cargo.toml**：版本号 `0.1.6` → `0.1.7`

## [0.1.6] - 2026-05-29

### Fixed

- **剔除 evidence 字段的强制检查**（`card_lint.rs`）
  - 根因：12 个 prompt 文件均未要求 `evidence` 字段，但代码对所有卡片强制检查 `evidence` 是否为空，导致 504 张卡片全部被过滤（0 张通过）。
  - 修复：注释掉 `check_evidence_traceability()` 调用；从 `is_valid` 致命问题列表中移除 `MissingEvidence` 和 `EvidenceNotFound`。
  - 影响：evidence 恢复为可选增强字段，有则加分，无则不影响卡片通过。

- **放宽"标题与内容不匹配"检查**（`card_lint.rs`）
  - 根因：原逻辑为纯字符级精确匹配（要求标题关键词完整出现在内容中），但 LLM 生成的标题是对内容的概括，措辞不同是正常现象。导致 154 张卡片被误报为"不匹配"。
  - 修复：新策略为"去除所有中英文标点后，按2-6字滑动窗口检查"——只要标题中有任意2字以上连续片段出现在内容中，即判定为匹配。优先检查较长片段（6字→2字），减少误判。
  - 影响：大幅降低误报率，同时保留对严重离题标题的检测能力。

- **改进信息密度计算算法**（`card_lint.rs`）
  - 根因：原算法仅统计 47 个固定标记词，只覆盖学术写作风格，导致人物卡、金句卡等高质量卡片被误判为"信息密度低"。
  - 修复：加权多维度算法——学术术语(2.0分)、引用标记(1.5分)、量化标记(1.5分)、逻辑连接词(1.0分)、结构化标记(0.5分)；标记词扩展至 90+ 个；阈值从 0.1 放宽至 0.05；扣分从 0.2 降至 0.1。
  - 影响：大幅降低信息密度误报率。

- **金句卡豁免 `LikelyCopied` 检查**（`card_lint.rs`）
  - 根因：金句卡 prompt 要求包含原文（`original_text`），content 以"原文：..."开头是正常格式，不应判定为"直接复制原文"。
  - 修复：`check_reference_consistency()` 中对金句卡跳过相似度检查。
  - 影响：消除金句卡的 100% 相似度误报。

- **放宽综述卡跨主题连接检查**（`card_lint.rs`）
  - 根因：原逻辑要求综述卡必须包含"综合/关联/跨/整体/一方面/另一方面"等特定词汇，但 LLM 可能用其他方式实现跨主题整合。字符串包含检查无法判断语义层面的连接。
  - 修复：剔除关键词检查，仅保留字数门槛（≥120字）。
  - 影响：消除综述卡的关键词误报。

- **修复 ref 字段提取与格式规范**（`cards.rs` + 12 个 prompt）
  - 根因1（字段名不匹配）：prompt 要求 LLM 输出 `ref：[来源]`，但代码提取的是 `extract_field(block, "参考")`，当 LLM 按 prompt 输出 `ref：` 时完全提取不到。
  - 根因2（格式不明确）：prompt 只说 `ref：[来源]`，没有指定具体格式，导致 LLM 输出五花八门的 ref：`作者简介`（无页码）、`《态度改变与社会影响》`（原始出处而非当前文档）、`文档第236页`（"文档"不是书名）。
  - 修复：代码改为 `extract_field(block, "ref").or_else(|| extract_field(block, "参考"))`，同时支持两种字段名；12 个 prompt 统一将 `ref：` 改为 `参考：`；全局 prompt 新增"参考字段格式规范"，明确要求格式为 `阳志平《人生模式》| {章节名} | 第{x}页`，必须引用当前文档而非原始出处。
  - 影响：ref 格式统一，可追溯性增强。

### Changed
- **Cargo.toml**：版本号 `0.1.5` → `0.1.6`

### Added
- **./documents/ 文件夹**：存放用户上传/要求处理的原始文档
  - 命名规则：`{日期}{文档名}`（无下划线），如 `20260529143000人生模式.pdf`
  - 编译完成后自动复制原始文档到此文件夹

### Changed
- **输出目录命名修复**：`20260529_105112_人生模式` → `20260529105112_人生模式`
  - `TIMESTAMP_FORMAT` 从 `%Y%m%d_%H%M%S` 改为 `%Y%m%d%H%M%S`

### Removed
- **删除按章节拆分编译**：`compile_by_chapters()` 已移除
  - 恢复为一个文档一个文件夹的简洁结构
  - 输出目录内只含 `all_cards.md`（全书卡片汇总）

## [0.1.4] - 2026-05-29

### Added
- **保存原始文档副本**：每次编译后自动保存原始文件到输出目录
  - `save_source_docs()`：保存原始文件副本（`source_文件名`）和 Markdown 文本（`_source_text.md`）
  - 支持所有编译路径（全书编译、按章节编译、策展模式）
  - 解决用户忘记生成了哪些文档的问题

### Changed
- **公开内部函数**：`save_source_copy()` 新增于 `output.rs`

## [0.1.3] - 2026-05-29

### Added
- **按章节拆分编译 PDF**：PDF 有目录结构时，自动按章节拆分，每章独立编译
  - 新增 `compile_by_chapters()` 函数（`main.rs`）
  - 提取 PDF 目录（TOC），按一级标题拆分章节
  - 每章保存到独立子目录：`{timestamp}_{书名}/{章节名}/`
  - 所有章节卡片汇总到顶层 `all_cards.md`
  - 支持容错：单章编译失败不影响其他章节

- **精确的 ref 引用信息**：卡片引用自动补充书名、章节、页码
  - 格式：`《书名》| 章节标题 | 第x-y页`
  - 解决"看不出引用来源"的问题

### Changed
- **目录命名优先使用文件名**：`save_single()` 中目录名优先从 `source_file` 提取
  - 优先：PDF 文件名（去掉扩展名）
  - 回退：LLM 生成的 `summary.title`
  - 解决 LLM 生成标题不准确导致的目录名问题

- **公开必要的内部函数**：为 `compile_by_chapters` 提供支持
  - `converter.rs`: `split_pdf_by_toc()`、`save_pdf_range()` → `pub`
  - `output.rs`: `create_output_dir()`、`save_single_to_dir()`、`sanitize_filename()` → `pub`

## [0.1.2] - 2026-05-28

### Fixed
- **Mermaid 中文实体安全性修复**：`mermaid_escape()` 函数安全转义 Mermaid 语法敏感字符
  - 转义 `"` → `'`（避免中断字符串）
  - 转义 `#` → `\#`（避免被误认为注释）
  - 转义 `\r`, `\n`, `\t` → 空格（避免破坏单行语法）
  - 节点标签和关系文本均应用转义
  - 新增 7 个边界测试覆盖中文特殊字符场景

- **Card Markdown 字段命名对齐**：`to_default_markdown()` / `to_quote_markdown()` 输出字段统一
  - `**参考：**` → `**ref：**`
  - `**唯一编码：**` → `**uuid：**`
  - 与 P2 字段命名统一（prompts 已改，代码输出未改）对齐

- **`total_fixes` 逻辑修复**：`output.rs:380` 变量声明后永不修改
  - `typo_fix()` 改为返回 `usize`（修复数量）
  - `save_cards_by_type()` 累加修复数量，排版修复消息现在正确输出

- **curation 子文档时间戳冲突修复**：`create_output_dir()` 同一秒内重复调用时目录名冲突
  - 新增 `resolve_unique_output_dir()` + `resolve_unique_output_dir_raw()`
  - 目录名冲突时自动追加 `_001`, `_002` 递增序号
  - 新增 4 个边界测试（单次冲突、多次冲突、序号间隙复用）

- **`CARDNOTE_PROMPTS_DIR` 环境变量**：支持 prompts 目录自定义位置
  - 优先级：$CARDNOTE_PROMPTS_DIR > exe 位置推断 > 工作目录 ./prompts
  - 解决二进制单独分发时 prompts 找不到的问题

- **策展模式输出路径对齐**：`compile_book()` 新增 `output_dir` 参数
  - 替换 `pipeline.rs:356` 硬编码的 `"./output"`
  - 现在尊重 `--output` / `-o` CLI 参数

## [0.1.1] - 2026-05-27

### Added
- **PDF Expert OCR 支持**：新增 pdf-expert-batch-ocr 集成（macOS），高质量扫描版 PDF 识别
  - 自动检测 pdf-expert-batch-ocr 项目位置（环境变量 → 相对路径 → HOME 标准位置）
  - 优先级：pdf-expert-batch-ocr > MinerU > pytesseract
  - 支持自动回退，确保处理连续性

### Changed
- **隐私边界修复**：环境变量化所有硬编码路径
  - `find_mineru()`：检查 `$MINERU_PATH` 环境变量，支持 PATH 查询
  - `find_ocr_project()`：检查 `$CARDNOTE_OCR_PROJECT_PATH` 环境变量，多位置自动探测
  - 移除源码中的用户路径痕迹

- **卡片生成质量对齐**（阿志平卡片大法）
  - **P0 核心认知动作修复**：各卡片类型必含核心字段
    - 新知卡：已知 → 新知 → 例子
    - 事件卡：时间 + 地点 + 行动者 + 行动 + 反应/结果
    - 术语卡：定义 + 解释 + 例子
    - 人物卡：小传 + 主要贡献/研究方向 + 代表作品
    - 行动卡：原理 + 可执行动作
    - 金句卡：仿写型 + 评论型（双路径支持）
    - 基础卡：原文 + 收获/评论
    - 图示卡：可视化强制要求（Mermaid/ASCII/图片）
  - **P1 质量检查扩展**：覆盖所有 12 种卡片类型
    - 必填字段检查（标题、ref、uuid、#卡片类型）
    - 核心字段完整性检查
    - 认知动作校验（保留类型特有的思维转变）
  - **P2 字段命名统一**：
    - 汉字字段名 → 英文（ref、uuid）
    - 所有 prompts 文件标准化

### Fixed
- 隐私泄露风险：用户路径从源码中移除
- 卡片结构不完整：确保每种卡片类型的必填字段齐全
- OCR 工具链缺口：添加 macOS PDF Expert 高质量选项

### Technical
- 新增 `read_pdf_expert_ocr()` 函数（converter.rs）：batch_ocr.py 集成
- 新增 `read_pdf_scan_fallback_mineru()` 函数（converter.rs）：MinerU 独立实现
- 公开 `find_ocr_project()` 函数（scan.rs）：供 converter 调用
- 导入 `find_ocr_project`（converter.rs）：支持项目位置自动探测

### Not Included (v0.2.0+)
- Mermaid 中文实体安全性修复
- curation 子文档多重时间戳问题修复
- 完整配置系统与诊断工具统一
- 测试合约与 CI/CD 集成

---

## [0.1.0] - 2026-05-xx

### Initial Release
- PDF/Markdown → Markdown 文本转换
- MinerU + PyMuPDF + pytesseract 多层 OCR 回退
- LLM 摘要、实体提取、卡片生成、图谱构建
- Map-Reduce 长文档处理
- 基础卡片输出与质量报告
