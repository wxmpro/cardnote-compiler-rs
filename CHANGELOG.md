# Changelog

All notable changes to cardnote-compiler-rs are documented here.

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
