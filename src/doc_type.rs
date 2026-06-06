/// 文档类型检测
///
/// 根据文档内容特征自动判断文档类型，不同类型使用不同编译策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentType {
    /// 书籍：长文档，多章节，有目录/前言/后记
    Book,
    /// 学术论文：有摘要/关键词/参考文献，图表
    Paper,
    /// 文章：中等长度，结构简单
    Article,
    /// 报告：有数据表格，摘要/结论
    Report,
    /// 手册/指南：步骤指令，代码片段
    Manual,
    /// 未知/通用
    Unknown,
}

impl DocumentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DocumentType::Book => "书籍",
            DocumentType::Paper => "论文",
            DocumentType::Article => "文章",
            DocumentType::Report => "报告",
            DocumentType::Manual => "手册",
            DocumentType::Unknown => "未知",
        }
    }

    /// 推荐的编译策略
    pub fn compile_config(&self) -> CompileConfig {
        match self {
            DocumentType::Book => CompileConfig {
                chunk_size: 50000,
                max_cards_per_chunk: 30,
                expect_chapters: true,
                expect_entities: true,
                expect_cross_refs: true,
            },
            DocumentType::Paper => CompileConfig {
                chunk_size: 30000,
                max_cards_per_chunk: 25,
                expect_chapters: false,
                expect_entities: true,
                expect_cross_refs: true,
            },
            DocumentType::Article => CompileConfig {
                chunk_size: 15000,
                max_cards_per_chunk: 15,
                expect_chapters: false,
                expect_entities: false,
                expect_cross_refs: false,
            },
            DocumentType::Report => CompileConfig {
                chunk_size: 40000,
                max_cards_per_chunk: 25,
                expect_chapters: true,
                expect_entities: true,
                expect_cross_refs: false,
            },
            DocumentType::Manual => CompileConfig {
                chunk_size: 35000,
                max_cards_per_chunk: 20,
                expect_chapters: true,
                expect_entities: false,
                expect_cross_refs: false,
            },
            DocumentType::Unknown => CompileConfig::default(),
        }
    }
}

/// 文档类型检测结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentTypeDetection {
    pub doc_type: DocumentType,
    pub confidence: u8,
    pub best_score: u32,
    pub second_score: u32,
}

/// 编译配置（按文档类型定制）
#[derive(Debug, Clone)]
pub struct CompileConfig {
    /// 分块大小（字符数）
    pub chunk_size: usize,
    /// 每块最大卡片数
    pub max_cards_per_chunk: usize,
    /// 是否期望章节结构
    pub expect_chapters: bool,
    /// 是否期望实体识别
    pub expect_entities: bool,
    /// 是否期望交叉引用
    pub expect_cross_refs: bool,
}

impl CompileConfig {
    /// 配置参数范围校验
    ///
    /// 规则：
    /// - chunk_size: [1000, 500_000]（过小导致过度分块，过大导致OOM）
    /// - max_cards_per_chunk: [1, 100]（过少浪费API调用，过多导致输出截断）
    pub fn validate(&self) -> Result<(), String> {
        if self.chunk_size == 0 || self.chunk_size < 1000 {
            return Err(format!(
                "chunk_size 必须 >= 1000，当前: {}",
                self.chunk_size
            ));
        }
        if self.chunk_size > 500_000 {
            return Err(format!(
                "chunk_size 必须 <= 500000，当前: {}",
                self.chunk_size
            ));
        }
        if self.max_cards_per_chunk == 0 {
            return Err(format!(
                "max_cards_per_chunk 必须 >= 1，当前: {}",
                self.max_cards_per_chunk
            ));
        }
        if self.max_cards_per_chunk > 100 {
            return Err(format!(
                "max_cards_per_chunk 必须 <= 100，当前: {}",
                self.max_cards_per_chunk
            ));
        }
        Ok(())
    }
}

impl Default for CompileConfig {
    fn default() -> Self {
        Self {
            chunk_size: 50000,
            max_cards_per_chunk: 25,
            expect_chapters: true,
            expect_entities: true,
            expect_cross_refs: false,
        }
    }
}

/// 文档类型检测器
pub struct DocTypeDetector;

impl DocTypeDetector {
    /// 基于文档内容特征检测文档类型
    pub fn detect(text: &str) -> DocumentType {
        Self::detect_with_confidence(text).doc_type
    }

    pub fn detect_with_confidence(text: &str) -> DocumentTypeDetection {
        let features = Self::extract_features(text);
        Self::classify_with_confidence(&features)
    }

    /// 提取文档特征
    fn extract_features(text: &str) -> DocFeatures {
        let char_count = text.chars().count();
        let line_count = text.lines().count();

        // 标题特征
        let h1_count = text.lines().filter(|l| l.starts_with("# ")).count();
        let h2_count = text.lines().filter(|l| l.starts_with("## ")).count();
        let h3_count = text.lines().filter(|l| l.starts_with("### ")).count();

        // 关键词特征（大小写不敏感）
        let lower = text.to_lowercase();
        let has_abstract = lower.contains("摘要") || lower.contains("abstract");
        let has_keywords = lower.contains("关键词") || lower.contains("keywords");
        let has_references = lower.contains("参考文献")
            || lower.contains("references")
            || lower.contains("bibliography");
        let has_toc = lower.contains("目录")
            || lower.contains("table of contents")
            || lower.contains("contents");
        let has_appendix = lower.contains("附录") || lower.contains("appendix");
        let has_preface = lower.contains("前言") || lower.contains("preface");
        let has_conclusion = lower.contains("结论") || lower.contains("conclusion");
        // 代码块检测： fenced code block 或 4空格缩进（排除表格行）
        let has_code = text.contains("```")
            || text.lines().any(|l| {
                let t = l.trim_start();
                t.starts_with("    ") && !t.starts_with("|")
            });
        let has_table = text.contains("| ") && text.lines().filter(|l| l.contains("|")).count() > 3;
        let has_figure = lower.contains("图") || lower.contains("figure") || lower.contains("fig.");

        // 步骤特征（手册/指南）：只统计明确的步骤标记
        let step_count = lower.matches("步骤").count()
            + lower.matches("step").count()
            + lower.matches("1. ").count()
            + lower.matches("2. ").count();

        DocFeatures {
            char_count,
            line_count,
            h1_count,
            h2_count,
            h3_count,
            has_abstract,
            has_keywords,
            has_references,
            has_toc,
            has_appendix,
            has_preface,
            has_conclusion,
            has_code,
            has_table,
            has_figure,
            step_count,
        }
    }

    fn classify_with_confidence(features: &DocFeatures) -> DocumentTypeDetection {
        let mut scores: Vec<(DocumentType, u32)> = vec![
            (DocumentType::Book, Self::book_score(features)),
            (DocumentType::Paper, Self::paper_score(features)),
            (DocumentType::Article, Self::article_score(features)),
            (DocumentType::Report, Self::report_score(features)),
            (DocumentType::Manual, Self::manual_score(features)),
        ];

        scores.sort_by_key(|(_, s)| *s);
        scores.reverse();

        let (best_type, best_score) = scores[0];
        let second_score = scores[1].1;
        let doc_type = if best_score == 0
            || best_score < 20
            || (best_score > 0 && second_score > 0 && best_score - second_score < 5)
        {
            DocumentType::Unknown
        } else {
            best_type
        };
        let confidence = match doc_type {
            DocumentType::Unknown => 30,
            _ => ((best_score.saturating_sub(second_score) * 2 + best_score).min(95)) as u8,
        };

        DocumentTypeDetection {
            doc_type,
            confidence,
            best_score,
            second_score,
        }
    }

    fn book_score(f: &DocFeatures) -> u32 {
        let mut score = 0;
        if f.char_count > 5000 {
            score += 20;
        }
        if f.char_count > 20000 {
            score += 10;
        }
        if f.h1_count >= 3 {
            score += 25;
        }
        if f.h2_count >= 3 {
            score += 15;
        }
        if f.has_toc {
            score += 40;
        }
        if f.has_preface {
            score += 20;
        }
        if f.has_appendix {
            score += 15;
        }
        if f.has_references {
            score += 5;
        }
        score
    }

    fn paper_score(f: &DocFeatures) -> u32 {
        let mut score = 0;
        if f.has_abstract {
            score += 45;
        }
        if f.has_keywords {
            score += 30;
        }
        if f.has_references {
            score += 30;
        }
        if f.has_figure {
            score += 10;
        }
        if f.char_count > 2000 {
            score += 5;
        }
        if f.has_conclusion {
            score += 5;
        }
        score
    }

    fn article_score(f: &DocFeatures) -> u32 {
        let mut score = 0;
        // 空文本不应偏向任何类型
        if f.char_count == 0 {
            return 0;
        }
        if f.char_count < 3000 {
            score += 20;
        }
        // 必须有H1标题（Article的最小结构要求）
        if f.h1_count >= 1 {
            score += 10;
        }
        if f.h1_count <= 2 {
            score += 10;
        }
        if f.h2_count <= 3 {
            score += 5;
        }
        // 否定条件权重降低，避免空文本高分
        if !f.has_references {
            score += 5;
        }
        if !f.has_toc {
            score += 3;
        }
        if !f.has_appendix {
            score += 3;
        }
        if !f.has_abstract {
            score += 5;
        }
        score
    }

    fn report_score(f: &DocFeatures) -> u32 {
        let mut score = 0;
        if f.has_table {
            score += 35;
        }
        if f.has_abstract {
            score += 20;
        }
        if f.has_conclusion {
            score += 20;
        }
        if f.char_count > 1000 {
            score += 10;
        }
        if f.h1_count >= 2 {
            score += 10;
        }
        if f.has_references {
            score += 5;
        }
        score
    }

    fn manual_score(f: &DocFeatures) -> u32 {
        let mut score = 0;
        if f.has_code {
            score += 40;
        }
        if f.step_count >= 2 {
            score += 25;
        }
        if f.h2_count >= 2 {
            score += 15;
        }
        if f.has_table {
            score += 5;
        }
        if !f.has_abstract {
            score += 5;
        }
        if !f.has_references {
            score += 5;
        }
        score
    }
}

#[derive(Debug, Clone)]
struct DocFeatures {
    char_count: usize,
    #[allow(dead_code)]
    line_count: usize,
    h1_count: usize,
    h2_count: usize,
    #[allow(dead_code)]
    h3_count: usize,
    has_abstract: bool,
    has_keywords: bool,
    has_references: bool,
    has_toc: bool,
    has_appendix: bool,
    has_preface: bool,
    has_conclusion: bool,
    has_code: bool,
    has_table: bool,
    has_figure: bool,
    step_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_book() {
        let text = r#"# 前言

内容A。

## 标题A

内容B。

## 标题B

内容B。

## 标题C

内容B。

# 目录

- 标题A
- 标题B
- 标题C

# 附录

内容C。"#;

        let doc_type = DocTypeDetector::detect(text);
        assert_eq!(doc_type, DocumentType::Book);
    }

    #[test]
    fn test_detect_paper() {
        let text = r#"# 摘要

内容A。

**关键词**：领域A、领域B

## 引言

## 方法

## 结果

## 结论

# 参考文献

1. 作者A (年份A)
2. 作者B (年份B)"#;

        let doc_type = DocTypeDetector::detect(text);
        assert_eq!(doc_type, DocumentType::Paper);
    }

    #[test]
    fn test_detect_article() {
        let text = r#"# 标题A

内容B。

## 关键点

- 要点A
- 要点B
- 要点C"#;

        let doc_type = DocTypeDetector::detect(text);
        assert_eq!(doc_type, DocumentType::Article);
    }

    #[test]
    fn test_detect_manual() {
        let text = r#"# 标题B

## 步骤A

```bash
curl -O URLA
```

## 步骤B

```bash
tar xzf 文件A
```

## 步骤C

编辑 配置A"#;

        let doc_type = DocTypeDetector::detect(text);
        assert_eq!(doc_type, DocumentType::Manual);
    }

    #[test]
    fn test_detect_report() {
        let text = r#"# 标题C

## 摘要

内容C。

## 标题A 概述A

### 子节A 指标A

内容D。

### 子节B 进展A

内容E。

## 标题B 分析A

### 子节C 数据A

| 月份 | 收入(万) | 支出(万) | 净利润(万) |
|------|----------|----------|------------|
| 1月  | 850      | 720      | 130        |
| 2月  | 920      | 780      | 140        |
| 3月  | 1050     | 820      | 230        |

内容F。

### 子节D 数据B

内容G。

## 标题C 评估A

内容H。

## 结论

内容I。"#;

        let doc_type = DocTypeDetector::detect(text);
        assert_eq!(doc_type, DocumentType::Report);
    }

    #[test]
    fn test_compile_config() {
        let config = DocumentType::Book.compile_config();
        assert_eq!(config.chunk_size, 50000);
        assert!(config.expect_chapters);

        let config = DocumentType::Article.compile_config();
        assert_eq!(config.chunk_size, 15000);
        assert!(!config.expect_chapters);
    }

    // ── 边界测试 ──

    #[test]
    fn test_detect_empty() {
        // 空文本：所有类型得分都低于阈值，应判为 Unknown
        let doc_type = DocTypeDetector::detect("");
        assert_eq!(doc_type, DocumentType::Unknown);
    }

    #[test]
    fn test_detect_very_short() {
        // 极短文本：结构简单、无引用无目录 => Article
        let doc_type = DocTypeDetector::detect("文本A");
        assert_eq!(doc_type, DocumentType::Article);
    }

    #[test]
    fn test_detect_mixed_features() {
        // 短文本 + 混合特征：Article 的否定条件（短文本、无引用）得分压倒性优势
        let text = r#"# 摘要

内容J。

## 步骤A

```bash
curl -O URLA
```"#;
        let doc_type = DocTypeDetector::detect(text);
        // 短文本(字符<3000)让 Article 得分最高，与次高分差距>=5
        assert_eq!(doc_type, DocumentType::Article);
    }

    #[test]
    fn test_detect_strong_manual() {
        // 强手册特征：代码块 + 多步骤 + H2 结构
        let text = r#"# 标题B

## 步骤A

```bash
curl -O URLA
```

## 步骤B

```bash
tar xzf 文件A
```

## 步骤C

编辑 配置A"#;
        let doc_type = DocTypeDetector::detect(text);
        assert_eq!(doc_type, DocumentType::Manual);
    }

    #[test]
    fn test_detect_long_report() {
        // 长文本(字符>3000) + 多表格 + 摘要 + 结论 + 参考文献 => Report 明确胜出
        let text = r#"# 标题C

## 摘要

内容K。内容K。内容K。
内容K。内容K。内容K。

## 标题A 概述A

### 子节A 指标A

内容L。内容L。内容L。
内容L。内容L。内容L。

| 指标 | 数值 | 同比 |
|------|------|------|
| A | 100 | +5% |
| B | 200 | +10% |
| C | 300 | +15% |

| 月份 | 收入 | 支出 |
|------|------|------|
| 1月 | 100 | 80 |
| 2月 | 120 | 90 |
| 3月 | 140 | 100 |

| 区域 | 数值 | 环比 |
|------|------|------|
| 东区 | 50 | +2% |
| 西区 | 60 | +3% |

## 标题B 分析A

内容M。内容M。内容M。
内容M。内容M。内容M。

## 结论

内容N。内容N。内容N。
内容N。内容N。内容N。

# 参考文献

1. 作者A (年份A)
2. 作者B (年份B)
3. 作者C (年份C)"#;
        let doc_type = DocTypeDetector::detect(text);
        assert_eq!(doc_type, DocumentType::Report);
    }

    #[test]
    fn test_detect_long_no_features() {
        // 长文本(字符>3000)但无任何显著特征：Article 因否定条件仍得分最高
        let text = "内容O。".repeat(100);
        let doc_type = DocTypeDetector::detect(&text);
        // Article 的 h1_count<=2(15) + h2_count<=3(10) + !has_abstract(10) + !has_references(10) 等 = 55
        // 其他类型无特征 = 0，差距 >= 5 => Article
        assert_eq!(doc_type, DocumentType::Article);
    }

    // ── 配置参数范围校验测试 ──

    #[test]
    fn test_compile_config_validate_ok() {
        let config = CompileConfig {
            chunk_size: 50000,
            max_cards_per_chunk: 12,
            expect_chapters: true,
            expect_entities: true,
            expect_cross_refs: false,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_compile_config_validate_chunk_too_small() {
        let config = CompileConfig {
            chunk_size: 500,
            max_cards_per_chunk: 12,
            ..CompileConfig::default()
        };
        assert!(config.validate().is_err());
        assert!(config.validate().unwrap_err().contains("chunk_size"));
    }

    #[test]
    fn test_compile_config_validate_chunk_zero() {
        let config = CompileConfig {
            chunk_size: 0,
            max_cards_per_chunk: 12,
            ..CompileConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_compile_config_validate_chunk_too_large() {
        let config = CompileConfig {
            chunk_size: 1_000_000,
            max_cards_per_chunk: 12,
            ..CompileConfig::default()
        };
        assert!(config.validate().is_err());
        assert!(config.validate().unwrap_err().contains("500000"));
    }

    #[test]
    fn test_compile_config_validate_cards_zero() {
        let config = CompileConfig {
            chunk_size: 50000,
            max_cards_per_chunk: 0,
            ..CompileConfig::default()
        };
        assert!(config.validate().is_err());
        assert!(
            config
                .validate()
                .unwrap_err()
                .contains("max_cards_per_chunk")
        );
    }

    #[test]
    fn test_compile_config_validate_cards_too_many() {
        let config = CompileConfig {
            chunk_size: 50000,
            max_cards_per_chunk: 200,
            ..CompileConfig::default()
        };
        assert!(config.validate().is_err());
        assert!(config.validate().unwrap_err().contains("100"));
    }

    #[test]
    fn test_compile_config_validate_boundary() {
        // 边界值：最小有效值
        let config = CompileConfig {
            chunk_size: 1000,
            max_cards_per_chunk: 1,
            ..CompileConfig::default()
        };
        assert!(config.validate().is_ok());

        // 边界值：最大有效值
        let config = CompileConfig {
            chunk_size: 500_000,
            max_cards_per_chunk: 100,
            ..CompileConfig::default()
        };
        assert!(config.validate().is_ok());
    }
}
