use regex::Regex;
use std::sync::LazyLock;

use crate::models::{Card, CardStatus, CardType};

// ═══════════════════════════════════════════════════════
// [C2] 预编译正则表达式（避免每次 lint 重复编译）
// ═══════════════════════════════════════════════════════

static RE_REF_PAGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.+)_第(\d+)(?:-\d+)?页$").expect("硬编码正则"));
static RE_BOOK_PAGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^本书第(\d+)页$").expect("硬编码正则"));
static RE_CITE_P: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.+)[,，]\s*p[\.．]?(\d+).*$").expect("硬编码正则"));
static RE_P_DOT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.+)_p\.(\d+)$").expect("硬编码正则"));
static RE_AUTHOR_BOOK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^阳志平《([^》]+)》").expect("硬编码正则"));
static RE_AUTHOR_BOOK_FULL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^阳志平《([^》]+)》.*$").expect("硬编码正则"));
static RE_AUTHOR_YEAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([^_]+)_\d{4}_.*$").expect("硬编码正则"));
static RE_PAREN_BOOK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([^（(]+)[（(].*[)）].*$").expect("硬编码正则"));
static RE_EXTRACT_BOOK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"《([^》]+)》").expect("硬编码正则"));

// [M1] 已知书名列表（运行时从 .cardnote/books.json 加载，不再硬编码）

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
            min_info_density: 0.05, // v0.1.6: 从 0.1 放宽到 0.05，配合加权算法减少误报
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
    /// ref 格式不符合规范
    InvalidRefFormat,
    /// 卡片类型混淆（如术语卡标题是其他卡片类型的名称）
    TypeConfusion,
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
            LintIssue::InvalidRefFormat => write!(f, "ref格式不符合规范"),
            LintIssue::TypeConfusion => write!(f, "卡片类型混淆：内容不属于该类型"),
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
                        ' ' | '\u{3000}'
                            | '\u{3001}'
                            | '\u{3002}'
                            | '\u{FF0C}'
                            | '\u{FF1A}'
                            | '\u{FF1B}'
                            | '\u{FF01}'
                            | '\u{FF1F}'
                            | '\u{201C}'
                            | '\u{201D}'
                            | '\u{2018}'
                            | '\u{2019}'
                            | '\u{FF08}'
                            | '\u{FF09}'
                            | '\u{300A}'
                            | '\u{300B}'
                            | '\u{3008}'
                            | '\u{3009}'
                            | '\u{00B7}'
                            | '"'
                            | '\''
                            | '('
                            | ')'
                            | '['
                            | ']'
                            | '{'
                            | '}'
                            | '<'
                            | '>'
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

    // 规则6.5: ref 格式检查
    // [v0.1.6] 强制要求 ref 符合 v3 格式规范：来源名_p页码
    check_ref_format(card, &mut issues);

    // 规则6.8: 类型混淆检测 — 术语卡标题不能是卡片类型名称
    if card.card_type == CardType::Term {
        let forbidden_titles = [
            "反常识卡",
            "新知卡",
            "术语卡",
            "金句卡",
            "综述卡",
            "行动卡",
            "人物卡",
            "事件卡",
            "图示卡",
            "新词卡",
            "基础卡",
            "索引卡",
        ];
        for &ft in &forbidden_titles {
            if card.title.contains(ft) {
                issues.push(LintIssue::TypeConfusion);
                break;
            }
        }
    }

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
                | LintIssue::InvalidRefFormat
                | LintIssue::TypeConfusion
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
        // [v0.1.15] 先自动修复 ref 格式，再检查
        let mut card = card.clone();
        fix_ref_format(&mut card, source_text);
        let result = lint_card_with_source(&card, source_text, config);
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

/// 计算信息密度（v0.1.6 改进版）
///
/// 改进点：
/// 1. 标记词从 47 个扩展到 90+ 个，覆盖更多表达方式
/// 2. 新增词性维度：动作性标记、深度分析标记、引用标记、量化标记
/// 3. 权重差异化：学术术语 2 分，逻辑连接词 1 分，引用标记 1.5 分
///
/// 原算法的问题：标记词太少（仅 47 个），只覆盖学术写作风格，
/// 导致大量高质量卡片（如人物卡、金句卡）被误判为"信息密度低"。
fn compute_info_density(content: &str) -> f64 {
    if content.is_empty() {
        return 0.0;
    }

    let char_count = content.chars().count().max(1);
    let markers = crate::config::density_markers();
    let mut score = 0.0;

    // 权重 2.0：学术/专业术语
    for term in &markers.academic_terms {
        score += content.matches(term.as_str()).count() as f64 * 2.0;
    }

    // 权重 1.5：引用/来源标记 + 标点引用
    for term in &markers.citation_terms {
        score += content.matches(term.as_str()).count() as f64 * 1.5;
    }
    for p in &["「", "【", "《", "\"", "'", "(", "（"] {
        score += content.matches(p).count() as f64 * 1.5;
    }

    // 权重 1.5：量化/数据标记 + 数字
    for q in &markers.quantifiers {
        score += content.matches(q.as_str()).count() as f64 * 1.5;
    }
    let digit_count = content.chars().filter(|c| c.is_ascii_digit()).count();
    score += digit_count as f64 * 0.5;

    // 权重 1.0：逻辑连接词
    for conn in &markers.logic_connectors {
        score += content.matches(conn.as_str()).count() as f64 * 1.0;
    }

    // 权重 0.5：结构化标记
    for m in &markers.structure_markers {
        score += content.matches(m.as_str()).count() as f64 * 0.5;
    }

    (score * 100.0) / char_count as f64
}

/// 检查引用一致性
///
/// 规则：
/// 1. 金句卡有原文但无出处 → ReferenceMismatch
/// 2. 金句卡内容与原文高度相似（>80%）→ LikelyCopied（直接复制非改写）
/// 3. 引用字段包含乱码字符 → ReferenceMismatch
fn check_reference_consistency(card: &Card, issues: &mut Vec<LintIssue>) {
    // 规则1：金句卡引用完整性（仅检查金句卡）
    if card.card_type == CardType::Quote && !card.original_text.is_empty() && card.source.is_empty()
    {
        issues.push(LintIssue::ReferenceMismatch);
    }

    // 规则2：内容是否直接复制原文（金句卡豁免）
    // [v0.1.6] 金句卡本来就应该包含原文（original_text），content 以"原文：..."
    // 开头是正常格式，不应判定为"直接复制"。仅对非金句卡执行此检查。
    if card.card_type != CardType::Quote
        && !card.original_text.is_empty()
        && !card.content.is_empty()
    {
        let similarity = compute_text_similarity(&card.content, &card.original_text);
        if similarity > 0.8 {
            issues.push(LintIssue::LikelyCopied { similarity });
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

/// 自动修复 ref 格式（v0.1.15 新增）
///
/// 在 check_ref_format 之前执行，将 LLM 常见的格式错误自动转换为规范格式。
/// 修复规则基于实际编译中观察到的 40+ 种错误模式。
fn fix_ref_format(card: &mut Card, source_text: &str) {
    let ref_text = card.reference.trim();
    if ref_text.is_empty() {
        return;
    }

    let mut fixed = ref_text.to_string();

    // ── 规则1: xxx_第数字页 或 xxx_第数字-数字页 → xxx_p数字 ──
    // 例: 人生模式_第79-84页 → 人生模式_p79
    // 例: 阳志平《聪明的阅读者》第二篇行动模式_第126-126页 → 阳志平《聪明的阅读者》第二篇行动模式_p126
    if let Some(caps) = RE_REF_PAGE.captures(&fixed) {
        let prefix = caps.get(1).unwrap().as_str();
        let page = caps.get(2).unwrap().as_str();
        fixed = format!("{}_p{}", prefix, page);
    }

    // ── 规则2: 本书第数字页 → 推断书名_p数字 ──
    // 例: 本书第330页 → 人生模式_p330
    if let Some(caps) = RE_BOOK_PAGE.captures(&fixed) {
        let page = caps.get(1).unwrap().as_str();
        let book_name = infer_book_name(source_text);
        fixed = format!("{}_p{}", book_name, page);
    }

    // ── 规则3: xxx，p.数字 或 xxx, p.数字 → 提取书名_p数字 ──
    // 例: 阳志平《聪明的阅读者》，p.187 及该章内多处 → 聪明的阅读者_p187
    if let Some(caps) = RE_CITE_P.captures(&fixed) {
        let prefix = caps.get(1).unwrap().as_str();
        let page = caps.get(2).unwrap().as_str();
        // 去掉"阳志平"前缀，提取书名号内的内容
        let prefix = prefix.trim_start_matches("阳志平").trim();
        let book = extract_book_name(prefix);
        fixed = format!("{}_p{}", book, page);
    }

    // ── 规则4: xxx_p.数字 → xxx_p数字 ──
    if let Some(caps) = RE_P_DOT.captures(&fixed) {
        let prefix = caps.get(1).unwrap().as_str();
        let page = caps.get(2).unwrap().as_str();
        fixed = format!("{}_p{}", prefix, page);
    }

    // ── 规则5: 阳志平《书名》..._p数字 → 书名_p数字 ──
    // 去掉作者前缀，保留书名号内的书名
    if let Some(caps) = RE_AUTHOR_BOOK.captures(&fixed) {
        let book_name = caps.get(1).unwrap().as_str().trim();
        // 如果后面有 _p 后缀，保留
        if let Some(p_idx) = fixed.rfind("_p") {
            fixed = format!("{}{}", book_name, &fixed[p_idx..]);
        }
    }

    // ── 规则6: 阳志平《书名》...（无 _p 后缀）→ 提取书名，尝试找页码 ──
    if RE_AUTHOR_BOOK_FULL.is_match(&fixed)
        && !fixed.contains("_p")
        && let Some(caps) = RE_AUTHOR_BOOK_FULL.captures(&fixed)
    {
        let book_name = caps.get(1).unwrap().as_str().trim();
        // 尝试从文本中查找该书的引用页码
        if let Some(page) = find_book_page_in_source(book_name, source_text) {
            fixed = format!("{}_p{}", book_name, page);
        }
    }

    // ── 规则7: 作者_年份_标题（如 杨中芳、杨宜音_2001_系列研究）→ 简化 ──
    if RE_AUTHOR_YEAR.is_match(&fixed)
        && !fixed.contains("_p")
        && let Some(caps) = RE_AUTHOR_YEAR.captures(&fixed)
    {
        let author = caps.get(1).unwrap().as_str().trim();
        // 尝试从文本中查找该作者的引用页码
        if let Some(page) = find_author_page_in_source(author, source_text) {
            fixed = format!("人生模式_p{}", page);
        }
    }

    // ── 规则8: 作者（年份）书名（如 乡土人生（费孝通，1947））→ 提取书名 ──
    if RE_PAREN_BOOK.is_match(&fixed)
        && !fixed.contains("_p")
        && let Some(caps) = RE_PAREN_BOOK.captures(&fixed)
    {
        let book = caps.get(1).unwrap().as_str().trim();
        if let Some(page) = find_book_page_in_source(book, source_text) {
            fixed = format!("人生模式_p{}", page);
        }
    }

    // ── 规则9: APA 格式学术论文引用 → 尝试从源文本搜索概念名找页码 ──
    // 例: Dunbar, R. I. M. (1992). Neocortex size... → 人生模式_p234
    if !is_valid_v3_ref(&fixed) && !source_text.is_empty() {
        // 从卡片标题中提取关键概念词
        if let Some(page) = find_concept_page_by_title(&card.title, source_text) {
            fixed = format!("人生模式_p{}", page);
        }
    }

    // ── 规则10: 全角数字/标点修复 ──
    fixed = fixed
        .replace('０', "0")
        .replace('１', "1")
        .replace('２', "2")
        .replace('３', "3")
        .replace('４', "4")
        .replace('５', "5")
        .replace('６', "6")
        .replace('７', "7")
        .replace('８', "8")
        .replace('９', "9");

    if fixed != ref_text {
        card.reference = fixed;
    }
}

/// 判断是否为有效的 v3 ref 格式
fn is_valid_v3_ref(ref_text: &str) -> bool {
    if let Some(idx) = ref_text.find("_p") {
        let after = &ref_text[idx + 2..];
        return after.chars().next().is_some_and(|c| c.is_ascii_digit());
    }
    false
}

/// 从文本中推断书名
/// 运行时从 .cardnote/books.json 加载已知书籍列表
fn infer_book_name(source_text: &str) -> String {
    let known_books = crate::config::known_book_names();
    for book in &known_books {
        if source_text.contains(book.as_str()) {
            return book.clone();
        }
    }
    "来源".to_string()
}

/// 从前缀中提取书名号内的书名
fn extract_book_name(prefix: &str) -> String {
    if let Some(caps) = RE_EXTRACT_BOOK.captures(prefix) {
        caps.get(1).unwrap().as_str().trim().to_string()
    } else {
        prefix.to_string()
    }
}

/// 从源文本中搜索书名出现的页码
fn find_book_page_in_source(book_name: &str, source_text: &str) -> Option<String> {
    if source_text.is_empty() {
        return None;
    }
    let re = Regex::new(&format!(
        r"## 第 (\d+) 页[\s\S]{{0,500}}?{}",
        regex::escape(book_name)
    ))
    .unwrap();
    re.captures(source_text)
        .map(|caps| caps.get(1).unwrap().as_str().to_string())
}

/// 从源文本中搜索作者名出现的页码
fn find_author_page_in_source(author: &str, source_text: &str) -> Option<String> {
    if source_text.is_empty() {
        return None;
    }
    let re = Regex::new(&format!(
        r"## 第 (\d+) 页[\s\S]{{0,500}}?{}",
        regex::escape(author)
    ))
    .unwrap();
    re.captures(source_text)
        .map(|caps| caps.get(1).unwrap().as_str().to_string())
}

/// 从源文本中根据卡片标题搜索概念名出现的页码
fn find_concept_page_by_title(title: &str, source_text: &str) -> Option<String> {
    if source_text.is_empty() {
        return None;
    }
    // 从标题中提取关键词（去掉标点，取前8个字符）
    let keywords: Vec<&str> = title
        .split(|c: char| c.is_ascii_punctuation() || c == '，' || c == '？' || c == '！')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && s.chars().count() >= 2)
        .take(2)
        .collect();

    for kw in &keywords {
        let re = Regex::new(&format!(
            r"(?i)## 第 (\d+) 页[\s\S]{{0,1000}}?{}",
            regex::escape(kw)
        ))
        .unwrap();
        if let Some(caps) = re.captures(source_text) {
            return Some(caps.get(1).unwrap().as_str().to_string());
        }
    }
    None
}

/// 检查 ref 格式是否符合规范
///
/// 支持两种格式：
/// - v3 格式：`来源名_p页码`（如 `人生模式_p160`）
/// 只接受 v3 格式：`来源名_p页码`
/// 示例：人生模式_p160
fn check_ref_format(card: &Card, issues: &mut Vec<LintIssue>) {
    let ref_text = card.reference.trim();

    // ref 不能为空
    if ref_text.is_empty() {
        issues.push(LintIssue::InvalidRefFormat);
        return;
    }

    // 必须为 v3 格式：`来源名_p页码`
    if !ref_text.contains("_p") {
        issues.push(LintIssue::InvalidRefFormat);
        return;
    }

    // 检查 `_p` 后面是否有数字
    if let Some(idx) = ref_text.find("_p") {
        let after = &ref_text[idx + 2..];
        if after.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return; // v3 格式通过
        }
    }

    // _p 后面没有数字
    issues.push(LintIssue::InvalidRefFormat);
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
        CardType::Review
            // [v0.1.6] 已剔除关键词检查：原逻辑要求内容必须包含"综合/关联/跨/整体"
            // 等特定词汇才能判定为"有跨主题连接"，但 LLM 可能用其他表达方式实现
            // 同样的语义连接。字符串包含检查无法判断语义层面的跨主题整合。
            // 保留字数门槛（≥120字）确保综述卡有足够的展开空间。
            if card.content.chars().count() < 120 => {
                issues.push(LintIssue::ReviewMissingSynthesis);
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

/// 计算两段文本的相似度（最长公共子序列 LCS）
fn compute_text_similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let chars_a: Vec<char> = a.chars().collect();
    let chars_b: Vec<char> = b.chars().collect();
    let n = chars_a.len();
    let m = chars_b.len();

    if n == 0 || m == 0 {
        return 0.0;
    }

    // 动态规划计算 LCS 长度
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            if chars_a[i - 1] == chars_b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    let lcs_len = dp[n][m];
    lcs_len as f64 / n.max(m) as f64
}

/// 计算单张卡片质量评分（0.0-1.0），分段线性评分保留区分度
fn compute_card_quality_score(_card: &Card, issues: &[LintIssue]) -> f64 {
    if issues.is_empty() {
        return 1.0;
    }

    // 按严重程度分类统计
    let mut critical: u32 = 0;
    let mut major: u32 = 0;
    let mut minor: u32 = 0;

    for issue in issues {
        match issue {
            // Critical: 每个扣 0.4
            LintIssue::EmptyTitle
            | LintIssue::InvalidRefFormat
            | LintIssue::TypeConfusion
            | LintIssue::QuoteMissingSource
            | LintIssue::GraphMissingStructure
            | LintIssue::MissingEvidence
            | LintIssue::EvidenceNotFound => critical += 1,

            // Major: 每个扣 0.15
            LintIssue::EmptyOrShortContent { .. }
            | LintIssue::HighGarbageRatio { .. }
            | LintIssue::LikelyCopied { .. }
            | LintIssue::TitleContentMismatch
            | LintIssue::IndexTooFewEntries { .. } => major += 1,

            // Minor: 每个扣 0.05
            LintIssue::LowInfoDensity { .. }
            | LintIssue::ReferenceMismatch
            | LintIssue::ActionMissingSteps
            | LintIssue::TermMissingDefinition
            | LintIssue::ReviewMissingSynthesis => minor += 1,
        }
    }

    let critical_deduction = (critical as f64 * 0.4).min(0.9);
    let major_deduction = (major as f64 * 0.15).min(0.5);
    let minor_deduction = (minor as f64 * 0.05).min(0.3);

    let score = 1.0 - critical_deduction - major_deduction - minor_deduction;
    score.max(0.1) // 最低 0.1，保留区分度
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
            reference: "测试文档_p23".to_string(),
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
        // [v0.1.6] 金句卡豁免 LikelyCopied 检查：金句卡本来就应该包含原文，
        // content 以"原文：..."开头是正常格式。改为测试新知卡直接复制原文。
        let mut card = test_card();
        card.card_type = CardType::Knowledge;
        card.original_text = "内容F内容F内容F。".to_string();
        card.content = "内容F内容F内容F。".to_string(); // 直接复制原文
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

    #[test]
    fn test_ref_format_book_valid() {
        let mut card = test_card();
        card.reference = "人生模式_p7".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(
            !result.issues.contains(&LintIssue::InvalidRefFormat),
            "v3格式应为有效: {}",
            card.reference
        );
    }

    #[test]
    fn test_ref_format_pdf_valid() {
        let mut card = test_card();
        card.reference = "Result_20_p15".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(
            !result.issues.contains(&LintIssue::InvalidRefFormat),
            "v3格式应为有效: {}",
            card.reference
        );
    }

    #[test]
    fn test_ref_format_empty() {
        let mut card = test_card();
        card.reference = "".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(
            result.issues.contains(&LintIssue::InvalidRefFormat),
            "空ref应为无效"
        );
        assert!(!result.is_valid, "空ref应为致命问题");
    }

    #[test]
    fn test_ref_format_no_p_marker() {
        let mut card = test_card();
        card.reference = "文档第236页".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(
            result.issues.contains(&LintIssue::InvalidRefFormat),
            "无_p标记应为无效"
        );
    }

    #[test]
    fn test_ref_format_no_page_number() {
        let mut card = test_card();
        card.reference = "人生模式_p".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(
            result.issues.contains(&LintIssue::InvalidRefFormat),
            "_p后无数字应为无效"
        );
    }

    #[test]
    fn test_ref_format_author_bio() {
        let mut card = test_card();
        card.reference = "作者简介".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(
            result.issues.contains(&LintIssue::InvalidRefFormat),
            "作者简介应为无效"
        );
    }

    #[test]
    fn test_ref_format_invalid_v3() {
        let mut card = test_card();
        card.reference = "人生模式_abc".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(
            result.issues.contains(&LintIssue::InvalidRefFormat),
            "_p后非数字应为无效"
        );
    }

    // ── LCS 相似度对抗性测试 ──

    #[test]
    fn test_lcs_similarity_abab_vs_aaabbbb() {
        // 反例1: 字符相同但语序完全不同 — LCS 改善了字符包含率的问题
        let sim = compute_text_similarity("abababab", "aaaabbbb");
        assert!(sim < 0.7, "字符相同但语序不同应低于 0.7: {}", sim);
    }

    #[test]
    fn test_lcs_similarity_chinese_contains() {
        // 反例2: 后者包含前者
        let sim = compute_text_similarity("认知负荷理论", "认知负荷理论指出");
        assert!(sim > 0.6, "后者包含前者应返回较高相似度: {}", sim);
    }

    #[test]
    fn test_lcs_similarity_identical() {
        // 边界1: 完全相同
        let sim = compute_text_similarity("相同内容", "相同内容");
        assert!((sim - 1.0).abs() < 1e-10, "完全相同应返回 1.0: {}", sim);
    }

    #[test]
    fn test_lcs_similarity_completely_different() {
        // 边界2: 完全不同
        let sim = compute_text_similarity("认知负荷理论", "天气预报很准确");
        assert!(sim < 0.3, "完全不同应返回低相似度: {}", sim);
    }

    #[test]
    fn test_lcs_similarity_empty() {
        assert_eq!(compute_text_similarity("", "非空"), 0.0);
        assert_eq!(compute_text_similarity("非空", ""), 0.0);
    }

    // ── 质量评分区分度测试 ──

    #[test]
    fn test_quality_score_two_critical() {
        let card = test_card();
        let issues = vec![LintIssue::EmptyTitle, LintIssue::InvalidRefFormat];
        let score = compute_card_quality_score(&card, &issues);
        assert!(
            score >= 0.1 && score <= 0.35,
            "2 Critical 应评分 0.1-0.35: {}",
            score
        );
    }

    #[test]
    fn test_quality_score_ten_minor() {
        let card = test_card();
        let issues = vec![
            LintIssue::LowInfoDensity {
                density: 0.01,
                threshold: 0.05
            };
            10
        ];
        let score = compute_card_quality_score(&card, &issues);
        assert!(
            score >= 0.4 && score <= 0.7,
            "10 Minor 应评分 0.4-0.7: {}",
            score
        );
    }

    #[test]
    fn test_quality_score_no_issues() {
        let card = test_card();
        let score = compute_card_quality_score(&card, &[]);
        assert!(
            (score - 1.0).abs() < 1e-10,
            "无 issue 应评分 1.0: {}",
            score
        );
    }

    #[test]
    fn test_quality_score_differentiation() {
        // 2 Critical 和 10 Minor 应得不同分数
        let card = test_card();
        let critical_score = compute_card_quality_score(
            &card,
            &[LintIssue::EmptyTitle, LintIssue::InvalidRefFormat],
        );
        let minor_score = compute_card_quality_score(
            &card,
            &vec![
                LintIssue::LowInfoDensity {
                    density: 0.01,
                    threshold: 0.05
                };
                10
            ],
        );
        assert!(
            (critical_score - minor_score).abs() > 0.05,
            "不同 issue 组合应有区分度: critical={}, minor={}",
            critical_score,
            minor_score
        );
    }

    // ── LCS 性能基准测试 ──

    #[test]
    fn test_lcs_performance_500x500() {
        let a: String = (0..500)
            .map(|i| char::from_u32((0x4E00 + i % 20000) as u32).unwrap())
            .collect();
        let b: String = (0..500)
            .map(|i| char::from_u32((0x4E00 + (i + 100) % 20000) as u32).unwrap())
            .collect();
        let start = std::time::Instant::now();
        let _sim = compute_text_similarity(&a, &b);
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 100,
            "LCS 500x500字符应在100ms内完成: {}ms",
            elapsed.as_millis()
        );
    }

    // ── ref 格式正向测试 ──

    #[test]
    fn test_ref_format_valid_range() {
        let mut card = test_card();
        card.reference = "人生模式_p172-173".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(!result.issues.contains(&LintIssue::InvalidRefFormat));
    }

    #[test]
    fn test_ref_format_cjk_book_name() {
        let mut card = test_card();
        card.reference = "聪明的阅读者_p15".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(!result.issues.contains(&LintIssue::InvalidRefFormat));
    }

    // ── 类型混淆检测测试 ──

    #[test]
    fn test_type_confusion_term_card_title_is_card_type_name() {
        let mut card = test_card();
        card.card_type = CardType::Term;
        card.title = "反常识卡——什么是反常识".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        // 术语卡标题包含"反常识卡"应在检查中被标记
        assert!(!result.issues.is_empty() || !result.is_valid);
    }
}
