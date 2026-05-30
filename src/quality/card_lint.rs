use regex::Regex;

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
            min_info_density: 0.05,  // v0.1.6: 从 0.1 放宽到 0.05，配合加权算法减少误报
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

    // 规则6.5: ref 格式检查
    // [v0.1.6] 强制要求 ref 符合格式规范：
    // - 书籍：《书名》| 章节 | 第x页
    // - PDF/报告：《文档名》| 第x页
    check_ref_format(card, &mut issues);

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
    let mut score = 0.0;

    // 权重 2.0：学术/专业术语（信息密度最高的标记）
    let academic_terms = [
        "研究发现", "研究表明", "实验证明", "实验表明",
        "理论", "模型", "框架", "机制", "原理", "规律",
        "概念", "定义", "术语", "范式", "假设", "推论",
        "认知", "心理", "神经", "行为", "情绪", "动机",
        "结构", "系统", "模式", "流程", "算法", "函数",
    ];
    for term in &academic_terms {
        score += content.matches(term).count() as f64 * 2.0;
    }

    // 权重 1.5：引用/来源标记（表明有外部知识支撑）
    let citation_terms = [
        "提出", "指出", "认为", "主张", "强调", "总结",
        "引用", "借鉴", "参考", "依据", "根据", "基于",
    ];
    for term in &citation_terms {
        score += content.matches(term).count() as f64 * 1.5;
    }
    // 标点引用标记单独处理
    let citation_puncts = ["「", "【", "《", "\"", "'", "(", "（"];
    for p in &citation_puncts {
        score += content.matches(p).count() as f64 * 1.5;
    }

    // 权重 1.5：量化/数据标记（具体信息）
    let quantifiers = [
        "数据", "证据", "统计", "调查", "百分比", "比例",
        "数量", "数值", "指标", "维度", "程度", "水平",
        "大约", "约", "超过", "低于", "达到", "增至",
    ];
    for q in &quantifiers {
        score += content.matches(q).count() as f64 * 1.5;
    }
    // 数字字符单独计分
    let digit_count = content.chars().filter(|c| c.is_ascii_digit()).count();
    score += digit_count as f64 * 0.5;

    // 权重 1.0：逻辑连接词（论证结构）
    let logic_connectors = [
        "比如", "例如", "如", "像", "譬如",
        "首先", "其次", "再次", "最后", "第一", "第二", "第三",
        "因此", "所以", "因而", "从而", "于是",
        "然而", "但是", "不过", "却", "而", "反而",
        "虽然", "尽管", "即使", "纵然",
        "如果", "假设", "若", "只要", "只有", "那么", "则",
        "不仅", "不但", "而且", "并且", "同时", "此外", "另外",
        "因为", "由于", "鉴于", "考虑到",
        "不同于", "相较于", "相比", "相对", "相反", "反之",
        "分为", "包括", "涵盖", "包含", "涉及", "关于",
        "通过", "凭借", "利用", "采用", "运用", "使用",
        "导致", "造成", "引起", "引发", "产生", "带来",
        "影响", "作用", "效果", "结果", "后果", "成果",
        "区别", "差异", "区分", "辨别", "识别",
        "比较", "对比", "对照", "类比",
        "关键", "核心", "本质", "实质", "根本", "重点", "要点",
        "原因", "理由", "根源", "由来", "起因",
        "目的", "目标", "意图", "旨在", "为了",
        "意义", "价值", "重要性", "作用",
        "方法", "方式", "途径", "手段", "策略", "技巧", "步骤",
        "分析", "解析", "剖析", "解读", "阐释", "阐明", "论述", "论证",
        "总结", "归纳", "概括", "综述", "回顾", "梳理",
        "具体", "详细", "明确", "清晰", "确切", "明确",
        "实例", "案例", "事例", "例子", "样板", "典型",
    ];
    for conn in &logic_connectors {
        score += content.matches(conn).count() as f64 * 1.0;
    }

    // 权重 0.5：结构化标记（列表、分层）
    let structure_markers = [
        "：", ":", "1.", "2.", "3.", "4.", "5.",
        "一、", "二、", "三、", "四、", "五、",
        "（1）", "（2）", "（3）", "①", "②", "③",
    ];
    for m in &structure_markers {
        score += content.matches(m).count() as f64 * 0.5;
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
    if card.card_type == CardType::Quote {
        if !card.original_text.is_empty() && card.source.is_empty() {
            issues.push(LintIssue::ReferenceMismatch);
        }
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
    let re1 = Regex::new(r"^(.+)_第(\d+)(?:-\d+)?页$").unwrap();
    if let Some(caps) = re1.captures(&fixed) {
        let prefix = caps.get(1).unwrap().as_str();
        let page = caps.get(2).unwrap().as_str();
        fixed = format!("{}_p{}", prefix, page);
    }

    // ── 规则2: 本书第数字页 → 推断书名_p数字 ──
    // 例: 本书第330页 → 人生模式_p330
    let re2 = Regex::new(r"^本书第(\d+)页$").unwrap();
    if let Some(caps) = re2.captures(&fixed) {
        let page = caps.get(1).unwrap().as_str();
        let book_name = infer_book_name(source_text);
        fixed = format!("{}_p{}", book_name, page);
    }

    // ── 规则3: xxx，p.数字 或 xxx, p.数字 → 提取书名_p数字 ──
    // 例: 阳志平《聪明的阅读者》，p.187 及该章内多处 → 聪明的阅读者_p187
    let re3 = Regex::new(r"^(.+)[,，]\s*p[\.．]?(\d+).*$").unwrap();
    if let Some(caps) = re3.captures(&fixed) {
        let prefix = caps.get(1).unwrap().as_str();
        let page = caps.get(2).unwrap().as_str();
        // 去掉"阳志平"前缀，提取书名号内的内容
        let prefix = prefix.trim_start_matches("阳志平").trim();
        let book = extract_book_name(prefix);
        fixed = format!("{}_p{}", book, page);
    }

    // ── 规则4: xxx_p.数字 → xxx_p数字 ──
    let re4 = Regex::new(r"^(.+)_p\.(\d+)$").unwrap();
    if let Some(caps) = re4.captures(&fixed) {
        let prefix = caps.get(1).unwrap().as_str();
        let page = caps.get(2).unwrap().as_str();
        fixed = format!("{}_p{}", prefix, page);
    }

    // ── 规则5: 阳志平《书名》..._p数字 → 书名_p数字 ──
    // 去掉作者前缀，保留书名号内的书名
    let re5 = Regex::new(r"^阳志平《([^》]+)》").unwrap();
    if let Some(caps) = re5.captures(&fixed) {
        let book_name = caps.get(1).unwrap().as_str().trim();
        // 如果后面有 _p 后缀，保留
        if let Some(p_idx) = fixed.rfind("_p") {
            fixed = format!("{}{}", book_name, &fixed[p_idx..]);
        }
    }

    // ── 规则6: 阳志平《书名》...（无 _p 后缀）→ 提取书名，尝试找页码 ──
    let re6 = Regex::new(r"^阳志平《([^》]+)》.*$").unwrap();
    if re6.is_match(&fixed) && !fixed.contains("_p") {
        if let Some(caps) = re6.captures(&fixed) {
            let book_name = caps.get(1).unwrap().as_str().trim();
            // 尝试从文本中查找该书的引用页码
            if let Some(page) = find_book_page_in_source(book_name, source_text) {
                fixed = format!("{}_p{}", book_name, page);
            }
        }
    }

    // ── 规则7: 作者_年份_标题（如 杨中芳、杨宜音_2001_系列研究）→ 简化 ──
    let re7 = Regex::new(r"^([^_]+)_\d{4}_.*$").unwrap();
    if re7.is_match(&fixed) && !fixed.contains("_p") {
        if let Some(caps) = re7.captures(&fixed) {
            let author = caps.get(1).unwrap().as_str().trim();
            // 尝试从文本中查找该作者的引用页码
            if let Some(page) = find_author_page_in_source(author, source_text) {
                fixed = format!("人生模式_p{}", page);
            }
        }
    }

    // ── 规则8: 作者（年份）书名（如 乡土人生（费孝通，1947））→ 提取书名 ──
    let re8 = Regex::new(r"^([^（(]+)[（(].*[)）].*$").unwrap();
    if re8.is_match(&fixed) && !fixed.contains("_p") {
        if let Some(caps) = re8.captures(&fixed) {
            let book = caps.get(1).unwrap().as_str().trim();
            if let Some(page) = find_book_page_in_source(book, source_text) {
                fixed = format!("人生模式_p{}", page);
            }
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
    fixed = fixed.replace('０', "0").replace('１', "1").replace('２', "2")
        .replace('３', "3").replace('４', "4").replace('５', "5")
        .replace('６', "6").replace('７', "7").replace('８', "8")
        .replace('９', "9");

    if fixed != ref_text {
        card.reference = fixed;
    }
}

/// 判断是否为有效的 v3 ref 格式
fn is_valid_v3_ref(ref_text: &str) -> bool {
    if let Some(idx) = ref_text.find("_p") {
        let after = &ref_text[idx + 2..];
        return after.chars().next().map_or(false, |c| c.is_ascii_digit());
    }
    false
}

/// 从文本中推断书名
fn infer_book_name(source_text: &str) -> String {
    // 优先从文本中的标题提取
    if source_text.contains("人生模式") {
        "人生模式".to_string()
    } else if source_text.contains("聪明的阅读者") {
        "聪明的阅读者".to_string()
    } else {
        "来源".to_string()
    }
}

/// 从前缀中提取书名号内的书名
fn extract_book_name(prefix: &str) -> String {
    let re = Regex::new(r"《([^》]+)》").unwrap();
    if let Some(caps) = re.captures(prefix) {
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
    let re = Regex::new(&format!(r"## 第 (\d+) 页[\s\S]{{0,500}}?{}", regex::escape(book_name))).unwrap();
    re.captures(source_text)
        .map(|caps| caps.get(1).unwrap().as_str().to_string())
}

/// 从源文本中搜索作者名出现的页码
fn find_author_page_in_source(author: &str, source_text: &str) -> Option<String> {
    if source_text.is_empty() {
        return None;
    }
    let re = Regex::new(&format!(r"## 第 (\d+) 页[\s\S]{{0,500}}?{}", regex::escape(author))).unwrap();
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
        let re = Regex::new(&format!(r"(?i)## 第 (\d+) 页[\s\S]{{0,1000}}?{}", regex::escape(kw))).unwrap();
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
/// - 旧格式：《书名》| 章节名 | 第{x}页 或 《文档名》| 第{x}页
fn check_ref_format(card: &Card, issues: &mut Vec<LintIssue>) {
    let ref_text = card.reference.trim();

    // ref 不能为空
    if ref_text.is_empty() {
        issues.push(LintIssue::InvalidRefFormat);
        return;
    }

    // [v0.1.7] 自动将全角分隔符 ｜ 转换为半角 |
    let ref_text = if ref_text.contains('｜') {
        ref_text.replace('｜', "|")
    } else {
        ref_text.to_string()
    };

    // 检查是否为 v3 格式：`来源名_p页码`
    // 示例：人生模式_p160、阳志平《聪明的阅读者》
    let is_v3_format = ref_text.contains("_p");
    if is_v3_format {
        // v3 格式：只要包含 `_p` 就认为是合法的
        // 进一步检查 `_p` 后面是否有数字
        if let Some(idx) = ref_text.find("_p") {
            let after = &ref_text[idx + 2..];
            if after.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                return; // v3 格式通过
            }
        }
        // _p 后面没有数字，仍然算不完整的格式
        issues.push(LintIssue::InvalidRefFormat);
        return;
    }

    // 旧格式检查：《书名》| 章节 | 第x页

    // 必须包含书名号《...》
    let has_book_mark = ref_text.contains('《') && ref_text.contains('》');
    if !has_book_mark {
        issues.push(LintIssue::InvalidRefFormat);
        return;
    }

    // 必须包含页码标记"第...页"
    let has_page = ref_text.contains('第') && ref_text.contains('页');
    if !has_page {
        issues.push(LintIssue::InvalidRefFormat);
        return;
    }

    // 检查分隔符 | 的数量
    let pipe_count = ref_text.matches('|').count();
    if pipe_count != 1 && pipe_count != 2 {
        issues.push(LintIssue::InvalidRefFormat);
        return;
    }

    // 禁止词检查
    let banned = ["出处"];
    for b in &banned {
        if ref_text.contains(b) {
            issues.push(LintIssue::InvalidRefFormat);
            return;
        }
    }

    // 提取书名号中的内容，检查是否为空
    if let Some(start) = ref_text.find('《') {
        if let Some(end) = ref_text.find('》') {
            if start < end {
                let book_name = &ref_text[start + '《'.len_utf8()..end];
                if book_name.trim().is_empty() {
                    issues.push(LintIssue::InvalidRefFormat);
                }
            }
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
            // [v0.1.6] 已剔除关键词检查：原逻辑要求内容必须包含"综合/关联/跨/整体"
            // 等特定词汇才能判定为"有跨主题连接"，但 LLM 可能用其他表达方式实现
            // 同样的语义连接。字符串包含检查无法判断语义层面的跨主题整合。
            // 保留字数门槛（≥120字）确保综述卡有足够的展开空间。
            if card.content.chars().count() < 120 {
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
            LintIssue::LowInfoDensity { .. } => 0.1,      // 轻微（v0.1.6: 从 0.2 降到 0.1）
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
            LintIssue::InvalidRefFormat => 0.6, // 致命
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
            reference: "《测试文档》| 第23页".to_string(),
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
        card.reference = "《人生模式》| 导论 优化你的人生模式 | 第7页".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(
            !result.issues.contains(&LintIssue::InvalidRefFormat),
            "书籍格式应为有效: {}",
            card.reference
        );
    }

    #[test]
    fn test_ref_format_pdf_valid() {
        let mut card = test_card();
        card.reference = "《Result_20》| 第15页".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(
            !result.issues.contains(&LintIssue::InvalidRefFormat),
            "PDF格式应为有效: {}",
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
    fn test_ref_format_no_book_mark() {
        let mut card = test_card();
        card.reference = "文档第236页".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(
            result.issues.contains(&LintIssue::InvalidRefFormat),
            "无书名号应为无效"
        );
    }

    #[test]
    fn test_ref_format_no_page() {
        let mut card = test_card();
        card.reference = "《人生模式》| 导论".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(
            result.issues.contains(&LintIssue::InvalidRefFormat),
            "无页码应为无效"
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
    fn test_ref_format_wrong_pipe_count() {
        let mut card = test_card();
        card.reference = "《人生模式》| 导论 | 第7页 | 额外内容".to_string();
        let config = CardLintConfig::default();
        let result = lint_card(&card, &config);
        assert!(
            result.issues.contains(&LintIssue::InvalidRefFormat),
            "分隔符数量错误应为无效"
        );
    }
}
