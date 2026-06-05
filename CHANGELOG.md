# Changelog

All notable changes to cardnote-compiler-rs are documented here.

## [0.1.53] - 2026-06-05

### Changed

- **PDF/OCR 工具调用适配 conda 环境隔离** (`converter.rs`, `diagnostics.rs`)
  - 新增 `python_for_purpose()`：根据用途路由到正确的 conda 环境 python
    - `pymupdf`/`markitdown`/`batch_ocr`/`pytesseract` → `CARDNOTE_PYTHON_PADDLEOCR` 或自动检测 paddleocr 环境
    - `magic_pdf`/`mineru` → `CARDNOTE_PYTHON_MINERU` 或自动检测 mineru 环境
  - 新增 `find_magic_pdf()`：同时支持 `magic-pdf` CLI 和向后兼容 `mineru` CLI
    - 环境变量：`MAGIC_PDF_PATH`（新） > `MINERU_PATH`（兼容）
    - PATH 搜索：`magic-pdf` > `mineru`
    - conda 自动检测：`miniforge3`/`anaconda3`/`.conda` 的 `mineru` 环境
  - 所有 `Command::new("python3")` 调用点替换为对应环境的 python：
    - `read_pdf_raw()` → paddleocr 环境
    - `read_pdf_expert_ocr()` → paddleocr 环境
    - `read_pdf_ocr_fallback()` → paddleocr 环境
    - `split_pdf_by_toc()` → paddleocr 环境
    - `save_pdf_range()` → paddleocr 环境
    - `read_markitdown()` → paddleocr 环境
  - magic-pdf CLI 参数适配：移除旧版 `-b pipeline`，使用标准 `-m ocr -l ch`
  - `diagnostics.rs`：MinerU 模块检查改为 magic-pdf CLI 可用性检查

### Fixed
- **base 环境不再混装 magic-pdf**：卸载 base 环境 magic-pdf，避免与 paddleocr/OCR 工具链冲突
- **模型目录迁移**：`/tmp/mineru_models` → `~/miniforge3/envs/mineru/mineru_models`，避免 `/tmp` 被清理

---

## [0.1.23] - 2026-05-31

### Added

- **Stage 层重试（单轮编译）** (`pipeline.rs`)
  - `with_retry` 从编译块局部函数提升为模块级函数
  - `run_single` 中 summary/entities/cards/graph 四个阶段均获 3 次重试 + 指数退避
  - `diagnostics.StageFail.retry_count` 设为 3

- **集成测试** (`tests/integration_test.rs`)
  - Mock LLM 客户端实现 `ChatFn` / `ChatJsonFn` trait
  - 空结果健康检查测试
  - 加粗格式字段提取已知限制测试（3 个测试）

- **Provider 配置外部化** (`providers.rs`)
  - 首次运行时自动导出内置配置到 `~/.config/cardnote/providers.default.json`
  - 支持三层加载：内置默认 → 默认配置 → 用户覆盖
  - 用户只需在 `providers.json` 中写入需覆盖的 Provider

### Changed
- **Cargo.toml**：版本号 `0.1.22` → `0.1.23`

---

- **缓存文件夹自动清理** (`pipeline.rs`)
  - 每次启动自动删除 30 天前的缓存文件
  - 若文件数超过 200 自动删除最旧的

- **`handle_compile` 拆分** (`main.rs`)
  - `resolve_book_title()` 提取为独立函数

## [0.1.22] - 2026-05-31

### Fixed

- **修复中文文档卡片规划数量偏少** (`stages/cards.rs`)
  - 根因：`generate_cards()` 使用 `document.len()`（UTF-8 字节数）传给 `CardPlanner::plan()`，但 `CardPlanner` 以字符数判断阈值。中文文档字节数 ≈ 3× 字符数，导致接近阈值的中文文档被误判为小规模，卡片规划数量不足。
  - 修复：`document.len()` → `document.chars().count()`。
  - 影响：纯中文书籍（如《聪明的阅读者》）的卡片规划现在正确按字符数计算。

- **增强编译失败的可见性** (`main.rs`)
  - 根因：LLM 阶段失败时返回空值但不中断编译，用户看到"编译完成"但输出目录内是空文件。
  - 修复：在 `pipeline.run()` 返回后增加编译结果健康检查。如果摘要和卡片均为空，打印明确警告并提示检查 `compile_diagnostics.md`。
  - 行为：继续保存输出（让用户有诊断文件可看），但不再静默成功。

- **强化临时目录删除的路径安全检查** (`main.rs`)
  - 根因：`canonicalize` 失败时的回退逻辑只检查 `".."` 子串，可被 `"..."` 等形式绕过。
  - 修复：回退逻辑改为使用 `Path::components()` 遍历检查 `ParentDir` 组件。

### Changed
- **Cargo.toml**：版本号 `0.1.21` → `0.1.22`

---

## [0.1.15] - 2026-05-30

### Removed

- **删除策展功能**（多文档策展模式完全移除）
  - 删除 `prompts/book_curator.md`、`prompts/cross_document.md`
  - 删除 `src/pipeline.rs`：`compile_book()`、`discover_cross_relations()` 方法
  - 删除 `src/output.rs`：`CurationData` 结构体、`save_curation()` 函数
  - 删除 `src/main.rs`：`--book` CLI 参数、策展模式分支、相关 import

### Changed

- **Prompts 全面更新为 v3 版本**
  - 覆盖 24 个卡片类型中文 prompts（action_card, event_card, free_card, graph_card, index_card, knowledge_card, new_word_card, note_card, person_card, quote_card, technique_card, term_card 及各自英文版本）
  - 英文 prompts 移至 `test/en_prompts_v3/`，项目代码只保留中文版本
  - 原始 prompts 备份至 `test/prompts_backup/`
  - 旧版本非卡片 prompts（`all_cards.md`、`review_card.md`、`quality_check.md`）备份至 `test/`

- **质量报告位置修复**
  - 修改 `src/main.rs`：编译完成后自动将 `input_quality_report.md` 从临时目录移动到最终输出目录
  - 删除临时报告目录，确保一个 PDF 只输出一个文件夹

- **ref 格式自动修复（v0.1.15）**
  - 新增 `fix_ref_format()` 函数（`src/quality/card_lint.rs`），10 条自动修复规则：
    1. `xxx_第数字页` → `xxx_p数字`
    2. `xxx_第数字-数字页` → `xxx_p数字`
    3. `本书第数字页` → `推断书名_p数字`
    4. `xxx，p.数字` → `提取书名_p数字`
    5. `xxx_p.数字` → `xxx_p数字`
    6. `阳志平《书名》..._p数字` → `书名_p数字`
    7. `阳志平《书名》...` → `书名_p页码`
    8. `作者_年份_标题` → `人生模式_p页码`
    9. `书名（作者，年份）` → `人生模式_p页码`
    10. APA 学术论文引用 → `人生模式_p页码`（源文本搜索）
  - 自动修复在 lint 检查前执行，避免高质量卡片因 ref 格式问题被过滤

### Changed
- **Cargo.toml**：版本号 `0.1.14` → `0.1.15`

## [0.1.16] - 2026-05-30

### Changed

- **重写 4 个核心 prompts 为 v3 风格**
  - `entity_extraction.md`：增加 7 种实体类型定义、统一表述规则、拒绝标准、类型区分规则（72 行）
  - `quality_check.md`：增加检查维度表格、严重程度定义、一票否决拒绝标准（118 行）
  - `relation_graph.md`：增加 12 种关系类型表格、发现规则、关系合并规则（72 行）
  - `summary.md`：增加概述写作标准、核心要点选取标准、关键实体提取标准（72 行）

- **Cargo.toml**：版本号 `0.1.15` → `0.1.16`

## [0.1.15] - 2026-05-30

### Changed

- **重构书名提取逻辑**（`converter.rs` `extract_pdf_metadata`）
  - **删除策略2（文本搜索）**：原逻辑从文本中搜索 `"书名：XXX"` 或遍历前N行找第一个中文行，误报率极高，且与 `read_pdf_raw` 注入的 `## 第 X 页` 标记耦合产生污染。
  - **简化策略**：只剩两步——① PDF Info 字典 Title 字段（元数据，最权威）；② 文件名（用户已命名，次优 fallback）。
  - **删除死代码**：`extract_title_from_text` 函数及其 50+ 行文本搜索逻辑。
  - **保留 `looks_like_page_marker`**：继续用于校验 Info 字典的 Title 是否被 PDF 生成工具污染。
  - **结论**：书名只有两个可信来源——元数据 和 文件名。文本搜索不能作为书名提取手段。

### Changed
- **Cargo.toml**：版本号 `0.1.13` → `0.1.14`

## [0.1.13] - 2026-05-29

### Fixed

- **修复书名提取被页码标记污染**（`converter.rs` `extract_title_from_text`）
  - **根因**：`read_pdf_raw` 在每页文本前注入 `## 第 X 页` 标记作为分块锚点，`extract_title_from_text` 的模式2（遍历前50行找第一个中文行）匹配到了这个标记，导致目录名变成 `## 第 1 页`。
  - **修复1**：模式2 增加过滤条件——排除以 `#` 开头的 Markdown 标题、排除 `第 X 页/章/节/篇` 页码标记、排除出版社/印刷/版权/CIP 等 30+ 个非书名词汇。
  - **修复2**：扫描行数从 50 行扩展到 100 行，减少因前面内容多而错过书名的概率。
  - **修复3**：长度下限从 5 字符放宽到 3 字符（一些短书名如《论语》只有 2 字符，但 3 字符是合理的安全下限）。

### Changed
- **Cargo.toml**：版本号 `0.1.12` → `0.1.13`

## [0.1.12] - 2026-05-29

### Fixed

- **修复 PDF 书名提取误判**（`converter.rs` `extract_pdf_metadata`）
  - **根因**：`extract_pdf_metadata` 无条件信任 PDF Info 字典的 Title 字段，但 PDF 生成工具常把页码标记（如 `## 第 1 页`、`Page 1`）填入 Title，导致输出目录名变成乱码。
  - **修复**：提取 Title 后增加 `looks_like_page_marker()` 启发式校验，过滤以下模式：
    1. 以 `#` 开头（Markdown 标题标记）
    2. 包含 `第 x 页/章/节/篇`
    3. 英文 `Page/Chapter/Section/Part + 数字`
    4. 纯数字
    5. 少于 3 个字符
  - **影响**：《阅读的心智》编译实测，目录名从 `2026..._## 第 1 页` 恢复为 `2026..._阅读的心智`。

### Changed
- **Cargo.toml**：版本号 `0.1.11` → `0.1.12`

## [0.1.11] - 2026-05-29

### Fixed

- **修复 documents/ 原始文档未复制**（`main.rs`）
  - **根因**：保存原始文档到 `./documents/` 的逻辑只写在 `save_single`（单文件）分支中，但分块编译的文档走的是 `save_book`（多 chunk）分支，导致 documents/ 始终为空。
  - **修复**：将文档复制逻辑提取到 `if/else` 分支**之前**，所有编译模式统一执行。
  - **同步修复**：文件名格式从 `{日期}{文档名}` 改为 `{日期}_{文档名}`（加下划线分隔符），解决 `20260529143000人生模式.pdf` 无法区分日期和文档名的问题。
  - **documents/README.md**：同步更新命名规则说明。

### Changed
- **Cargo.toml**：版本号 `0.1.10` → `0.1.11`

## [0.1.10] - 2026-05-29

### Fixed

- **修复输出目录名乱码**（`output.rs` `sanitize_filename`）
  - **根因**：函数只过滤了英文冒号 `:`（U+003A），但书名中的 `：` 是**中文全角冒号**（U+FF1A），未被处理。在文件系统创建目录时，中文冒号被错误编码成 `一^别` 等乱码字符。
  - **修复**：`sanitize_filename` 追加过滤中文全角符号：`：` `？` `"` `＜` `＞` `｜` `/` `\`。同时保留原有英文符号过滤。
  - **影响**：书名含中文冒号的 PDF（如《人生模式：识别并优化你的核心认知》）输出目录名不再出现乱码。

### Changed
- **Cargo.toml**：版本号 `0.1.9` → `0.1.10`

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
