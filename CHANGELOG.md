# Changelog

All notable changes to cardnote-compiler-rs are documented here.

## [0.1.5] - 2026-05-29

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
