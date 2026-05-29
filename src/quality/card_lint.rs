use crate::models::{Card, CardStatus, CardType};

/// 卡片质量检查配置
#[derive(Debug, Clone)]
pub struct CardLintConfig {
    /// 最小内容长度（字）
    pub min_content_length: usize,
    /// 最大乱码比例（0.0-1.0）
    pub max_garbage_ratio: f64,
    /// 最小信息密度（术语/概念标记每百字）
    pub min_info_density: f64,
    /// 是否检查引用一致性
    pub check_reference: bool,
}

impl Default for CardLintConfig {
    fn default() -> Self {
        Self {
            min_content_length: 30,
            max_garbage_ratio: 0.3,
            min_info_density: 0.1,
            check_reference: true,
        }
    }
}

/// 卡片质量问题
#[derive(Debug, Clone, PartialEq)]
pub enum LintIssue {
    /// 标题为空
    EmptyTitle,
    /// 内容为空或过短
    EmptyOrShortContent { actual: usize, required: usize },
    /// 内容乱码比例过高
    HighGarbageRatio { ratio: f64, threshold: f64 },
    /// 信息密度过低（可能是泛泛而谈）
    LowInfoDensity { density: f64, threshold: f64 },
    /// 引用与原文不一致
    ReferenceMismatch,
    /// 证据片段缺失
    MissingEvidence,
    /// 证据片段无法回到源文本
    EvidenceNotFound,
    /// 内容疑似直接复制原文（非改写）
    LikelyCopied { similarity: f64 },
    /// 标题与内容不匹配
    TitleContentMismatch,
    /// 金句卡缺少原文或出处
    QuoteMissingSource,
    /// 图示卡缺少可视化结构
    GraphMissingStructure,
    /// 行动卡缺少步骤或触发条件
    ActionMissingSteps,
    /// 术语卡缺少定义或例子
    TermMissingDefinition,
    /// 索引卡条目过少
    IndexTooFewEntries { actual: usize, required: usize },
    /// 综述卡缺少跨主题连接
    ReviewMissingSynthesis,
}

impl std::fmt::Display for LintIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LintIssue::EmptyTitle => write!(f, "标题为空"),
            LintIssue::EmptyOrShortContent { actual, required } => {
                write!(f, "内容过短: {}字 < 要求{}字", actual, required)
            }
            LintIssue::HighGarbageRatio { ratio, threshold } => {
                write!(
                    f,
                    "乱码比例过高: {:.1}% > {:.1}%",
                    ratio * 100.0,
                    threshold * 100.0
                )
            }
            LintIssue::LowInfoDensity { density, threshold } => {
                write!(f, "信息密度过低: {:.2} < {:.2}", density, threshold)
            }
            LintIssue::ReferenceMismatch => write!(f, "引用与原文不匹配"),
            LintIssue::MissingEvidence => write!(f, "证据片段缺失"),
            LintIssue::EvidenceNotFound => write!(f, "证据片段无法回到源文本"),
            LintIssue::LikelyCopied { similarity } => {
                write!(f, "疑似直接复制原文: 相似度{:.1}%", similarity * 100.0)
            }
            LintIssue::TitleContentMismatch => write!(f, "标题与内容不匹配"),
            LintIssue::QuoteMissingSource => write!(f, "金句卡缺少原文或出处"),
            LintIssue::GraphMissingStructure => write!(f, "图示卡缺少可视化结构"),
            LintIssue::ActionMissingSteps => write!(f, "行动卡缺少步骤或触发条件"),
            LintIssue::TermMissingDefinition => write!(f, "术语卡缺少定义或例子"),
            LintIssue::IndexTooFewEntries { actual, required } => {
                write!(f, "索引卡条目过少: {} < {}", actual, required)
            }
            LintIssue::ReviewMissingSynthesis => write!(f, "综述卡缺少跨主题连接"),
        }
    }
}

/// 卡片质量检查结果
#[derive(Debug, Clone)]
pub struct CardLintResult {
    pub card_title: String,
    pub issues: Vec<LintIssue>,
    pub is_valid: bool,
    pub quality_score: f64,
}

/// 质量过滤统计
#[derive(Debug, Clone, Default)]
pub struct LintStats {
    pub checked: usize,
    pub passed: usize,
    pub filtered: usize,
    pub issue_counts: std::collections::HashMap<String, usize>,
}

/// 检查单张卡片质量
pub fn lint_card(card: &Card, config: &CardLintConfig) -> CardLintResult {
    lint_card_with_source(card, "", config)
}

/// 结合源文本检查单张卡片质量
pub fn lint_card_with_source(
    card: &Card,
    _source_text: &str,
    config: &CardLintConfig,
) -> CardLintResult {
    let mut issues = Vec::new();

    // 规则1: 标题为空
    if card.title.trim().is_empty() {
        issues.push(LintIssue::EmptyTitle);
    }

    // 规则2: 内容为空或过短
    let content_len = card.content.chars().count();
    if content_len < config.min_content_length {
        issues.push(LintIssue::EmptyOrShortContent {
            actual: content_len,
            required: config.min_content_length,
        });
    }

    // 规则3: 乱码检测
    let garbage_ratio = compute_garbage_ratio(&card.content);
    if garbage_ratio > config.max_garbage_ratio {
        issues.push(LintIssue::HighGarbageRatio {
            ratio: garbage_ratio,
            threshold: config.max_garbage_ratio,
        });
    }

    // 规则4: 信息密度检测
    let density = compute_info_density(&card.content);
    if density < config.min_info_density && content_len >= 50 {
        issues.push(LintIssue::LowInfoDensity {
            density,
            threshold: config.min_info_density,
        });
    }

    // 规则5: 标题与内容匹配度（v0.1.6 放宽版）
    // LLM 生成的标题是对内容的概括，措辞不同是正常现象。
    // 原逻辑过于严格（要求关键词精确匹配），导致大量误报。
    // 新策略：去除所有中英文标点后，只要标题中有任意2字以上连续片段
    // 出现在内容中，即判定为匹配。
    if !card.title.is_empty() && !card.content.is_empty() {
        let title_clean = card.title.trim();
        let mut matched = false;

        // 策略1: 标题完整出现在内容中
        if card.content.contains(title_clean) {
            matched = true;
        } else {
            // 策略2: 去除所有中英文标点后，按2-6字滑动窗口检查
            // 优先检查较长片段（减少误判），逐步降级到2字片段
            let title_chars: Vec<char> = card
                .title
                .chars()
                .filter(|&c| {
                    // 保留中英文文字、数字；过滤常见标点
                    !matches!(
                        c,
                        ' ' | '\u{3000}' | '\u{3001}' | '\u{3002}' | '\u{FF0C}' | '\u{FF1A}'
                            | '\u{FF1B}' | '\u{FF01}' | '\u{FF1F}' | '\u{201C}' | '\u{201D}'
                            | '\u{2018}' | '\u{2019}' | '\u{FF08}' | '\u{FF09}' | '\u{300A}'
                            | '\u{300B}' | '\u{3008}' | '\u{3009}' | '\u{00B7}' | '"' | '\''
                            | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>'
                    )
                })
                .collect();

            for window in (2..=6).rev() {
                if title_chars.len() >= window {
                    for i in 0..=title_chars.len() - window {
                        let fragment: String = title_chars[i..i + window].iter().collect();
                        if card.content.contains(&fragment) {
                            matched = true;
                            break;
                        }
                    }
                    if matched {
                        break;
                    }
                }
            }
        }

        if !matched {
            issues.push(LintIssue::TitleContentMismatch);
        }
    }

    // 规则6: 引用一致性检查（金句卡专用 + 通用引用格式）
    check_reference_consistency(card, &mut issues);

    // 规则7: 类型化结构检查
    check_typed_card_requirements(card, &mut issues);

    // 规则8: 引用证据校验
    // [v0.1.6] 已禁用：12 个 prompt 均未要求 evidence 字段，
    // 不应作为强制检查项。evidence 为可选增强字段，有则加分，无则不影响通过。
    // check_evidence_traceability(card, source_text, &mut issues);

    // 计算质量评分（0.0-1.0）
    let quality_score = compute_card_quality_score(card, &issues);

    // 判定是否有效：致命问题直接过滤；其他问题允许通过但降低评分
    // [v0.1.6] 移除了 MissingEvidence 和 EvidenceNotFound：
    // prompt 未统一要求 evidence 字段，不应因此过滤卡片。
    let is_valid = !issues.iter().any(|i| {
        matches!(
            i,
            LintIssue::EmptyTitle
                | LintIssue::EmptyOrShortContent { .. }
                | LintIssue::QuoteMissingSource
                | LintIssue::GraphMissingStructure
                | LintIssue::IndexTooFewEntries { .. }
        )
    });

    CardLintResult {
        card_title: card.title.clone(),
        issues,
        is_valid,
        quality_score,
    }
}

/// 批量过滤卡片
pub fn filter_cards(cards: &[Card], config: &CardLintConfig) -> (Vec<Card>, LintStats) {
    filter_cards_with_source(cards, "", config)
}

/// 结合源文本批量过滤卡片
pub fn filter_cards_with_source(
    cards: &[Card],
    source_text: &str,
    config: &CardLintConfig,
) -> (Vec<Card>, LintStats) {
    let mut valid_cards = Vec::new();
    let mut stats = LintStats::default();

    for card in cards {
        stats.checked += 1;
        let result = lint_card_with_source(card, source_text, config);
        let mut checked_card = card.clone();
        checked_card.quality_score = result.quality_score;
        checked_card.status = if result.is_valid {
            CardStatus::Accepted
        } else if result
            .issues
            .iter()
            .any(|i| matches!(i, LintIssue::GraphMissingStructure))
        {
            checked_card.degraded_from = Some(checked_card.card_type.clone());
            checked_card.card_type = CardType::Knowledge;
            CardStatus::Degraded
        } else if result.issues.iter().any(|i| {
            matches!(
                i,
                LintIssue::ActionMissingSteps
                    | LintIssue::TermMissingDefinition
                    | LintIssue::ReviewMissingSynthesis
            )
        }) {
            checked_card.retry_count = checked_card.retry_count.saturating_add(1);
            CardStatus::NeedsRetry
        } else {
            CardStatus::Rejected
        };
        checked_card.reject_reason = result
            .issues
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("；");

        // 统计问题类型
        for issue in &result.issues {
            let key = issue.to_string();
            *stats.issue_counts.entry(key).or_insert(0) += 1;
        }

        if result.is_valid {
            stats.passed += 1;
            valid_cards.push(checked_card);
        } else {
            stats.filtered += 1;
        }
    }

    if stats.filtered > 0 {
        eprintln!(
            "  ✂️  质量过滤: {}/{} 张通过, {} 张被过滤",
            stats.passed, stats.checked, stats.filtered
        );
        // 打印被过滤的原因统计
        for (issue, count) in &stats.issue_counts {
            eprintln!("     - {}: {} 张", issue, count);
        }
    }

    (valid_cards, stats)
}

/// 计算乱码比例
/// 乱码特征：连续不可见字符、Unicode替换字符、异常标点比例
fn compute_garbage_ratio(text: &str) -> f64 {
    if text.is_empty() {
        return 0.0;
    }

    let mut garbage_chars = 0;
    let total_chars = text.chars().count();

    for ch in text.chars() {
        // 替换字符
        if ch == '\u{FFFD}' {
            garbage_chars += 1;
            continue;
        }
        // 控制字符（除常见换行/制表）
        if ch.is_control() && ch != '\n' && ch != '\t' && ch != '\r' {
            garbage_chars += 1;
            continue;
        }
        // 私人使用区字符
        if ('\u{E000}'..='\u{F8FF}').contains(&ch) || ('\u{F0000}'..='\u{FFFFD}').contains(&ch) {
            garbage_chars += 1;
        }
    }

    garbage_chars as f64 / total_chars.max(1) as f64
}

/// 计算信息密度
fn compute_info_density(content: &str) -> f64 {
    if content.is_empty() {
        return 0.0;
    }

    let char_count = content.chars().count().max(1);

    // 信息标记：术语括号、举例、逻辑连接词、学术概念标记
    let markers = content.matches('「').count()
        + content.matches('【').count()
        + content.matches("比如").count()
        + content.matches("例如").count()
        + content.matches("关键").count()
        + content.matches("核心").count()
        + content.matches("首先").count()
        + content.matches("其次").count()
        + content.matches("因此").count()
        + content.matches("然而").count()
        // 学术/论述标记
        + content.matches("研究发现").count()
        + content.matches("研究表明").count()
        + content.matches("实验").count()
        + content.matches("理论").count()
        + content.matches("模型").count()
        + content.matches("概念").count()
        + content.matches("定义").count()
        + content.matches("提出").count()
        + content.matches("指出").count()
        + content.matches("认为").count()
        // 结构化标记
        + content.matches("分为").count()
        + content.matches("包括").count()
        + content.matches("通过").count()
        + content.matches("基于").count()
        + content.matches("导致").count()
        + content.matches("影响").count()
        + content.matches("区别").count()
        + content.matches("比较").count()
        + content.matches("相反").count()
        + content.matches("不仅").count()
        + content.matches("而且").count()
        + content.matches("虽然").count()
        + content.matches("但是").count()
        + content.matches("如果").count()
        + content.matches("那么").count()
        + content.matches("不同于").count()
        + content.matches("相较于").count()
        // 数据/证据标记
        + content.matches("数据").count()
        + content.matches("证据").count()
        + content.matches("统计").count()
        + content.matches("调查").count()
        + content.matches("百分比").count()
        + content.matches("%").count()
        + content.matches("比例").count()
        // 英文学术标记（术语卡常见）
        + content.matches("vs").count()
        + content.matches("e.g.").count()
        + content.matches("i.e.").count();

    (markers as f64 * 100.0) / char_count as f64
}

/// 检查引用一致性
///
/// 规则：
/// 1. 金句卡有原文但无出处 → ReferenceMismatch
/// 2. 金句卡内容与原文高度相似（>80%）→ LikelyCopied（直接复制非改写）
/// 3. 引用字段包含乱码字符 → ReferenceMismatch
fn check_reference_consistency(card: &Card, issues: &mut Vec<LintIssue>) {
    // 规则1：金句卡引用完整性
    if card.card_type == CardType::Quote {
        if !card.original_text.is_empty() && card.source.is_empty() {
            issues.push(LintIssue::ReferenceMismatch);
        }

        // 规则2：内容是否直接复制原文
        if !card.original_text.is_empty() && !card.content.is_empty() {
            let similarity = compute_text_similarity(&card.content, &card.original_text);
            if similarity > 0.8 {
                issues.push(LintIssue::LikelyCopied { similarity });
            }
        }
    }

    // 规则3：引用字段格式检查
    if !card.reference.is_empty() {
        // 检查是否包含乱码
        let garbage_in_ref = card
            .reference
            .chars()
            .filter(|&c| {
                c == '\u{FFFD}' // 替换字符
                    || (c.is_control() && c != '\n' && c != '\t' && c != '\r')
            })
            .count();
        if garbage_in_ref > 0 {
            issues.push(LintIssue::ReferenceMismatch);
        }
    }
}

/// 检查类型化卡片结构要求
fn check_typed_card_requirements(card: &Card, issues: &mut Vec<LintIssue>) {
    match card.card_type {
        CardType::Quote => {
            if card.original_text.trim().is_empty() || card.source.trim().is_empty() {
                issues.push(LintIssue::QuoteMissingSource);
            }
        }
        CardType::Graph => {
            let has_mermaid = card.content.contains("graph ")
                || card.content.contains("flowchart")
                || card.content.contains("sequenceDiagram")
                || card.content.contains("mindmap");
            let has_structure = card.content.contains("->")
                || card.content.contains("-->")
                || card.content.contains("层级")
                || card.content.contains("流程")
                || card.content.contains("关系");
            if !has_mermaid && !has_structure {
                issues.push(LintIssue::GraphMissingStructure);
            }
        }
        CardType::Action => {
            let has_steps = card.content.contains("步骤")
                || card.content.contains("首先")
                || card.content.contains("其次")
                || card.content.contains("最后")
                || card.content.contains("1.")
                || card.content.contains("2.");
            let has_trigger = card.content.contains("当")
                || card.content.contains("如果")
                || card.content.contains("适用")
                || card.content.contains("场景")
                || card.content.contains("触发");
            if !has_steps && !has_trigger {
                issues.push(LintIssue::ActionMissingSteps);
            }
        }
        CardType::Term => {
            let has_definition = card.content.contains("是指")
                || card.content.contains("定义")
                || card.content.contains("概念")
                || card.content.contains("指的是")
                || card.content.contains("意味着");
            let has_example = card.content.contains("例如")
                || card.content.contains("比如")
                || card.content.contains("例子")
                || card.content.contains("表现为");
            if !has_definition && !has_example {
                issues.push(LintIssue::TermMissingDefinition);
            }
        }
        CardType::Index => {
            let entries = card
                .content
                .lines()
                .filter(|line| {
                    let trimmed = line.trim();
                    trimmed.starts_with('-')
                        || trimmed.starts_with('*')
                        || trimmed.starts_with(char::is_numeric)
                })
                .count();
            if entries < 3 {
                issues.push(LintIssue::IndexTooFewEntries {
                    actual: entries,
                    required: 3,
                });
            }
        }
        CardType::Review => {
            let has_synthesis = card.content.contains("综合")
                || card.content.contains("关联")
                || card.content.contains("跨")
                || card.content.contains("整体")
                || card.content.contains("一方面")
                || card.content.contains("另一方面");
            if card.content.chars().count() < 120 || !has_synthesis {
                issues.push(LintIssue::ReviewMissingSynthesis);
            }
        }
        _ => {}
    }
}

/// 检查证据可追溯性（v0.1.6 起暂停使用）
#[allow(dead_code)]
fn check_evidence_traceability(card: &Card, source_text: &str, issues: &mut Vec<LintIssue>) {
    if card.status == CardStatus::Rejected {
        return;
    }

    if card.evidence.trim().is_empty() {
        issues.push(LintIssue::MissingEvidence);
        return;
    }

    if !source_text.is_empty() && !source_text.contains(card.evidence.trim()) {
        issues.push(LintIssue::EvidenceNotFound);
    }
}

/// 计算两段文本的相似度（简单字符重叠率）
fn compute_text_similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    // 简化的相似度：较短的文本在较长文本中的最大连续子串匹配率
    let (shorter, longer) = if a.len() <= b.len() { (a, b) } else { (b, a) };

    // 计算shorter中有多少字符出现在longer中
    let shorter_chars: Vec<char> = shorter.chars().collect();
    let longer_chars: Vec<char> = longer.chars().collect();

    if shorter_chars.is_empty() {
        return 0.0;
    }

    let mut matched = 0;
    for ch in &shorter_chars {
        if longer_chars.contains(ch) {
            matched += 1;
        }
    }

    matched as f64 / shorter_chars.len() as f64
}

/// 计算单张卡片质量评分（0.0-1.0）
fn compute_card_quality_score(_card: &Card, issues: &[LintIssue]) -> f64 {
    let mut score: f64 = 1.0;

    // 每个问题按严重程度扣分
    for issue in issues {
        let deduction: f64 = match issue {
            LintIssue::EmptyTitle => 1.0,                 // 致命
            LintIssue::EmptyOrShortContent { .. } => 0.5, // 严重
            LintIssue::HighGarbageRatio { .. } => 0.4,    // 严重
            LintIssue::TitleContentMismatch => 0.3,       // 中等
            LintIssue::LowInfoDensity { .. } => 0.2,      // 轻微
            LintIssue::LikelyCopied { .. } => 0.25,       // 中等
            LintIssue::ReferenceMismatch => 0.15,         // 轻微
            LintIssue::MissingEvidence => 0.6,
            LintIssue::EvidenceNotFound => 0.6,
            LintIssue::QuoteMissingSource => 0.6,
            LintIssue::GraphMissingStructure => 0.6,
            LintIssue::IndexTooFewEntries { .. } => 0.6,
            LintIssue::ActionMissingSteps => 0.3,
            LintIssue::TermMissingDefinition => 0.3,
            LintIssue::ReviewMissingSynthesis => 0.3,
        };
        score -= deduction;
    }

    score.max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CardType;

    fn test_card() -> Card {
        Card {
            title: "标题A".to_string(),
            content: "内容A内容A内容A内容A内容A内容A内容A内容A内容A内容A。标题A。".to_string(),
            card_type: CardType::Knowledge,
            reference: "p.23".to_string(),
            unique_id: "20240101120000".to_string(),
            original_text: "".to_string(),
            source: "".to_string(),
            paraphrase: "".to_string(),
            related_cards: vec![],
            evidence: "内容A".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_lint_valid_card() {
        let card = test_card();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(result.is_valid);
        assert!(result.issues.is_empty());
        assert!(result.quality_score > 0.8);
    }

    #[test]
    fn test_lint_empty_title() {
        let mut card = test_card();
        card.title = "".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(!result.is_valid);
        assert!(result.issues.contains(&LintIssue::EmptyTitle));
    }

    #[test]
    fn test_lint_short_content() {
        let mut card = test_card();
        card.content = "短".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(!result.is_valid);
        assert!(
            result
                .issues
                .iter()
                .any(|i| matches!(i, LintIssue::EmptyOrShortContent { .. }))
        );
    }

    #[test]
    fn test_lint_garbage_content() {
        let mut card = test_card();
        card.content = "\u{FFFD}\u{FFFD}\u{FFFD}内容B\u{FFFD}\u{FFFD}".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        // 乱码比例超过阈值
        assert!(
            result
                .issues
                .iter()
                .any(|i| matches!(i, LintIssue::HighGarbageRatio { .. }))
        );
    }

    #[test]
    fn test_filter_cards() {
        let cards = vec![
            test_card(), // 有效
            Card {
                title: "".to_string(),
                content: "卡片A".to_string(),
                card_type: CardType::Knowledge,
                reference: "".to_string(),
                unique_id: "20240101120001".to_string(),
                ..Default::default()
            },
            Card {
                title: "标题B".to_string(),
                content: "短".to_string(),
                card_type: CardType::Term,
                reference: "".to_string(),
                unique_id: "20240101120002".to_string(),
                ..Default::default()
            },
        ];

        let config = CardLintConfig::default();
        let (valid, stats) = filter_cards(&cards, &config);
        assert_eq!(valid.len(), 1);
        assert_eq!(stats.checked, 3);
        assert_eq!(stats.passed, 1);
        assert_eq!(stats.filtered, 2);
    }

    #[test]
    fn test_title_content_mismatch() {
        let mut card = test_card();
        card.title = "标题X".to_string(); // 标题与内容完全无关
        card.content = "内容C内容C内容C内容C内容C内容C内容C内容C内容C内容C。".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        // 标题"概念D"与内容"概念C"不匹配
        assert!(result.issues.contains(&LintIssue::TitleContentMismatch));
    }

    #[test]
    fn test_garbage_ratio_calculation() {
        let normal = compute_garbage_ratio("内容D内容D内容D内容D内容D。");
        assert!(normal < 0.1);

        let garbage = compute_garbage_ratio("\u{FFFD}\u{FFFD}\u{FFFD}");
        assert!(garbage > 0.5);
    }

    #[test]
    fn test_quote_missing_source() {
        let mut card = test_card();
        card.card_type = CardType::Quote;
        card.original_text = "原文A原文A原文A。".to_string();
        card.source = "".to_string(); // 有原文但无出处
        card.content = "内容E内容E内容E。".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(result.issues.contains(&LintIssue::ReferenceMismatch));
    }

    #[test]
    fn test_quote_direct_copy() {
        let mut card = test_card();
        card.card_type = CardType::Quote;
        card.original_text = "内容F内容F内容F。".to_string();
        card.source = "出处A".to_string();
        card.content = "内容F内容F内容F。".to_string(); // 直接复制
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(
            result
                .issues
                .iter()
                .any(|i| matches!(i, LintIssue::LikelyCopied { .. }))
        );
    }

    #[test]
    fn test_quote_valid_paraphrase() {
        let mut card = test_card();
        card.card_type = CardType::Quote;
        card.original_text = "原文B原文B原文B。".to_string();
        card.source = "出处B".to_string();
        card.content = "内容G内容G内容G内容G内容G内容G。".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        // 有出处且有改写，不应触发引用问题
        assert!(!result.issues.contains(&LintIssue::ReferenceMismatch));
        assert!(
            !result
                .issues
                .iter()
                .any(|i| matches!(i, LintIssue::LikelyCopied { .. }))
        );
    }

    #[test]
    fn test_reference_garbage() {
        let mut card = test_card();
        card.reference = "p.\u{FFFD}23".to_string(); // 引用中包含乱码
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(result.issues.contains(&LintIssue::ReferenceMismatch));
    }

    // ── 边界测试 ──

    #[test]
    fn test_lint_empty_card() {
        let card = Card {
            title: "".to_string(),
            content: "".to_string(),
            card_type: CardType::Knowledge,
            reference: "".to_string(),
            unique_id: "".to_string(),
            ..Default::default()
        };
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        // 空卡片应有多个问题
        assert!(result.issues.contains(&LintIssue::EmptyTitle));
        assert!(
            result
                .issues
                .iter()
                .any(|i| matches!(i, LintIssue::EmptyOrShortContent { .. }))
        );
    }

    #[test]
    fn test_lint_very_long_content() {
        let mut card = test_card();
        card.content = "内容".repeat(5000);
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        // 长内容不应触发短内容警告
        assert!(
            !result
                .issues
                .iter()
                .any(|i| matches!(i, LintIssue::EmptyOrShortContent { .. }))
        );
    }

    #[test]
    fn test_lint_all_special_chars() {
        let mut card = test_card();
        // 控制字符会被视为垃圾字符
        card.content = "\x00\x01\x02\x03".repeat(50);
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        // 控制字符应触发乱码检测
        assert!(
            result
                .issues
                .iter()
                .any(|i| matches!(i, LintIssue::HighGarbageRatio { .. }))
        );
    }

    #[test]
    fn test_filter_cards_all_invalid() {
        let cards = vec![
            Card {
                title: "".to_string(),
                content: "".to_string(),
                card_type: CardType::Knowledge,
                ..Default::default()
            },
            Card {
                title: "".to_string(),
                content: "短".to_string(),
                card_type: CardType::Knowledge,
                ..Default::default()
            },
        ];
        let config = CardLintConfig::default();
        let result = filter_cards(&cards, &config);
        assert!(result.0.is_empty());
        assert_eq!(result.1.filtered, 2);
    }

    #[test]
    fn test_filter_cards_all_valid() {
        let cards = vec![test_card(), test_card()];
        let config = CardLintConfig::default();
        let result = filter_cards(&cards, &config);
        assert_eq!(result.0.len(), 2);
        assert_eq!(result.1.filtered, 0);
    }

    #[test]
    fn test_garbage_ratio_empty() {
        // 空文本的乱码比例由内部函数计算
    }

    #[test]
    fn test_garbage_ratio_all_garbage() {
        let text = "\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}";
        assert_eq!(compute_garbage_ratio(text), 1.0);
    }

    #[test]
    fn test_garbage_ratio_all_clean() {
        let text = "内容H内容H内容H内容H内容H。";
        assert_eq!(compute_garbage_ratio(text), 0.0);
    }
}
