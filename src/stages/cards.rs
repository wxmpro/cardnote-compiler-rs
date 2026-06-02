use std::sync::LazyLock;

use regex::Regex;

use crate::config::doc_limits_for;
use crate::doc_type::DocumentType;
use crate::error::Result;
use crate::models::{Card, CardStatus, CardType, LlmMessage};
use crate::stages::common::ChatFn;

/// 卡片规划条目
#[derive(Debug, Clone)]
pub struct CardPlanItem {
    /// 卡片类型
    pub card_type: CardType,
    /// 最少生成数量
    pub min: usize,
    /// 最多生成数量
    pub max: usize,
    /// 是否必须（必选类型 vs 可选类型）
    pub required: bool,
    /// 优先级（数值越小优先级越高）
    pub priority: usize,
}

impl CardPlanItem {
    pub fn new(
        card_type: CardType,
        min: usize,
        max: usize,
        required: bool,
        priority: usize,
    ) -> Self {
        Self {
            card_type,
            min,
            max,
            required,
            priority,
        }
    }
}

/// 卡片规划器
pub struct CardPlanner;

impl CardPlanner {
    /// 根据文档类型生成卡片规划
    pub fn plan(doc_type: DocumentType, char_count: usize) -> Vec<CardPlanItem> {
        match doc_type {
            DocumentType::Book => Self::plan_book(char_count),
            DocumentType::Paper => Self::plan_paper(char_count),
            DocumentType::Manual => Self::plan_manual(char_count),
            DocumentType::Report => Self::plan_report(char_count),
            DocumentType::Article => Self::plan_article(char_count),
            DocumentType::Unknown => Self::plan_default(char_count),
        }
    }

    /// 生成卡片规划文本（供 prompt 使用）
    pub fn plan_text(doc_type: DocumentType, char_count: usize) -> String {
        let items = Self::plan(doc_type, char_count);
        let mut lines = vec![
            format!("## 卡片生成规划（文档类型：{}）", doc_type.as_str()),
            format!("字数：约 {} 字符", char_count),
            String::new(),
            "### 必须生成的卡片类型：".to_string(),
        ];

        let required: Vec<_> = items.iter().filter(|i| i.required).collect();
        if required.is_empty() {
            lines.push("  （无强制要求）".to_string());
        } else {
            for item in &required {
                let type_name = Self::card_type_name(&item.card_type);
                lines.push(format!("  - {type_name}（{}-{} 张）", item.min, item.max));
            }
        }

        let optional: Vec<_> = items.iter().filter(|i| !i.required).collect();
        if !optional.is_empty() {
            lines.push(String::new());
            lines.push("### 可选生成的卡片类型：".to_string());
            for item in &optional {
                let type_name = Self::card_type_name(&item.card_type);
                lines.push(format!("  - {type_name}（{}-{} 张）", item.min, item.max));
            }
        }

        lines.push(String::new());
        lines.push("### 重要原则：".to_string());
        lines.push("  - 优先从「必须类型」中提取，确保每种至少生成 min 数量".to_string());
        lines.push("  - 可选类型根据文档内容是否有相关内容来决定是否生成".to_string());
        lines.push("  - 不要为了覆盖所有类型而硬凑——只生成真正有价值的卡片".to_string());
        lines.push("  - 如果某类内容在文档中明显缺失，直接跳过".to_string());

        lines.join("\n")
    }

    pub fn summary(doc_type: DocumentType, char_count: usize) -> String {
        let items = Self::plan(doc_type, char_count);
        let required = items
            .iter()
            .filter(|i| i.required)
            .map(|i| {
                format!(
                    "{} {}-{}张",
                    Self::card_type_name(&i.card_type),
                    i.min,
                    i.max
                )
            })
            .collect::<Vec<_>>()
            .join("、");
        let optional_count = items.iter().filter(|i| !i.required).count();
        format!(
            "{}；必选：{}；可选类型 {} 个",
            doc_type.as_str(),
            required,
            optional_count
        )
    }

    pub fn card_type_name(card_type: &CardType) -> &'static str {
        match card_type {
            CardType::Knowledge => "新知卡",
            CardType::Term => "术语卡",
            CardType::Person => "人物卡",
            CardType::Action => "行动卡",
            CardType::Quote => "金句卡",
            CardType::CounterIntuit => "反常识卡",
            CardType::Event => "事件卡",
            CardType::Graph => "图示卡",
            CardType::NewWord => "新词卡",
            CardType::Note => "基础卡",
            CardType::Index => "索引卡",
            CardType::Review => "综述卡",
        }
    }

    /// 书籍规划：多章节、综合性，优先新知卡、术语卡、综述卡、反常识卡
    fn plan_book(char_count: usize) -> Vec<CardPlanItem> {
        let scale = match char_count {
            0..=50000 => 1,
            50001..=150000 => 2,
            150001..=300000 => 3,
            300001..=500000 => 4,
            _ => (char_count / 100000).max(5),
        };
        vec![
            CardPlanItem::new(CardType::Knowledge, 3 * scale, 5 * scale, true, 1),
            CardPlanItem::new(CardType::Term, 2 * scale, 4 * scale, true, 2),
            CardPlanItem::new(CardType::Review, scale, 2 * scale, true, 3),
            CardPlanItem::new(CardType::CounterIntuit, scale, 3 * scale, true, 4),
            CardPlanItem::new(CardType::Action, 1, 3, false, 5),
            CardPlanItem::new(CardType::Person, 0, 2, false, 6),
            CardPlanItem::new(CardType::Quote, 0, 3, false, 7),
            CardPlanItem::new(CardType::Event, 0, 2, false, 8),
            CardPlanItem::new(CardType::Index, 0, 2, false, 9),
        ]
    }

    /// 论文规划：学术性强，优先术语卡、新知卡、反常识卡、综述卡
    fn plan_paper(char_count: usize) -> Vec<CardPlanItem> {
        let scale = match char_count {
            0..=30000 => 1,
            30001..=80000 => 2,
            80001..=150000 => 3,
            _ => 4,
        };
        vec![
            CardPlanItem::new(CardType::Term, 3 * scale, 5 * scale, true, 1),
            CardPlanItem::new(CardType::Knowledge, 2 * scale, 4 * scale, true, 2),
            CardPlanItem::new(CardType::CounterIntuit, 1, 3, true, 3),
            CardPlanItem::new(CardType::Review, 1, 2, true, 4),
            CardPlanItem::new(CardType::Quote, 0, 2, false, 5),
            CardPlanItem::new(CardType::Action, 0, 2, false, 6),
            CardPlanItem::new(CardType::Event, 0, 1, false, 7),
        ]
    }

    /// 手册规划：步骤指令多，优先行动卡、术语卡、图示卡、索引卡
    fn plan_manual(char_count: usize) -> Vec<CardPlanItem> {
        let scale = if char_count > 35000 { 2 } else { 1 };
        vec![
            CardPlanItem::new(CardType::Action, 3 * scale, 6 * scale, true, 1),
            CardPlanItem::new(CardType::Term, 2 * scale, 4 * scale, true, 2),
            CardPlanItem::new(CardType::Index, 1, 3, true, 3),
            CardPlanItem::new(CardType::Graph, 0, 2, false, 4),
            CardPlanItem::new(CardType::Knowledge, 0, 2, false, 5),
            CardPlanItem::new(CardType::Note, 0, 2, false, 6),
        ]
    }

    /// 报告规划：数据导向，优先术语卡、行动卡、图示卡、索引卡
    fn plan_report(char_count: usize) -> Vec<CardPlanItem> {
        let scale = if char_count > 40000 { 2 } else { 1 };
        vec![
            CardPlanItem::new(CardType::Term, 2 * scale, 4 * scale, true, 1),
            CardPlanItem::new(CardType::Action, 2 * scale, 4 * scale, true, 2),
            CardPlanItem::new(CardType::Graph, 0, 2, true, 3),
            CardPlanItem::new(CardType::Index, 0, 2, false, 4),
            CardPlanItem::new(CardType::Review, 0, 2, false, 5),
            CardPlanItem::new(CardType::Note, 0, 3, false, 6),
        ]
    }

    /// 文章规划：短小精悍，优先新知卡、金句卡、反常识卡
    fn plan_article(_char_count: usize) -> Vec<CardPlanItem> {
        vec![
            CardPlanItem::new(CardType::Knowledge, 1, 3, true, 1),
            CardPlanItem::new(CardType::Quote, 1, 2, true, 2),
            CardPlanItem::new(CardType::CounterIntuit, 0, 2, false, 3),
            CardPlanItem::new(CardType::Note, 0, 2, false, 4),
        ]
    }

    /// 未知/默认规划：保守策略
    fn plan_default(_char_count: usize) -> Vec<CardPlanItem> {
        vec![
            CardPlanItem::new(CardType::Knowledge, 1, 3, true, 1),
            CardPlanItem::new(CardType::Term, 0, 2, false, 2),
            CardPlanItem::new(CardType::Note, 0, 2, false, 3),
        ]
    }
}

/// 将 CardType 映射到 Prompt 文件名
fn card_type_prompt_name(card_type: &CardType) -> &'static str {
    match card_type {
        CardType::Knowledge => "knowledge_card",
        CardType::Term => "term_card",
        CardType::Person => "person_card",
        CardType::Quote => "quote_card",
        CardType::Event => "event_card",
        CardType::Action => "action_card",
        CardType::Graph => "graph_card",
        CardType::NewWord => "new_word_card",
        CardType::Note => "note_card",
        CardType::Index => "index_card",
        CardType::CounterIntuit => "counter_intuit_card",
        CardType::Review => "review_card",
    }
}

/// 对插入 prompt 的文档内容进行安全净化，防止 prompt 注入
fn sanitize_for_prompt(text: &str) -> String {
    let dangerous_patterns = [
        "ignore previous instructions",
        "ignore the above",
        "forget all rules",
        "忘记之前的指令",
        "忽略以上",
        "忽略上面的指令",
        "system:",
        "user:",
        "assistant:",
    ];

    let mut sanitized = text.to_string();
    for pattern in &dangerous_patterns {
        sanitized = sanitized.replace(pattern, &"█".repeat(pattern.len()));
    }

    // 限制最大长度（防止超长内容消耗 token）
    let max_len = 200_000; // 约 100K tokens
    if sanitized.len() > max_len {
        sanitized.truncate(max_len);
        sanitized.push_str("\n\n[内容已截断...]");
    }

    sanitized
}

/// 生成所有类型的卡片（混合策略：先尝试 Extract-Then-Assign，失败回退到分类型）
pub async fn generate_cards(
    document: &str,
    doc_type: DocumentType,
    call_llm: impl ChatFn,
    load_prompt: &(dyn Fn(&str) -> Result<String> + Send + Sync),
) -> Result<Vec<Card>> {
    let chars = document.chars().count();

    // 文档 ≤ 200K 字符时尝试新策略
    if chars <= 200_000 {
        match generate_cards_extract_then_assign(document, doc_type, &call_llm, load_prompt).await {
            Ok(cards) if cards.len() >= min_cards_threshold(doc_type, chars) => {
                eprintln!(
                    "    ✓ Extract-then-assign: {} 张卡片 (2次调用)",
                    cards.len()
                );
                return Ok(cards);
            }
            Ok(cards) => {
                eprintln!(
                    "    ⚠ Extract-then-assign 仅 {} 张 (<阈值), 回退到分类型策略",
                    cards.len()
                );
            }
            Err(e) => {
                eprintln!("    ⚠ Extract-then-assign 失败: {}, 回退到分类型策略", e);
            }
        }
    }

    generate_cards_legacy(document, doc_type, call_llm, load_prompt).await
}

/// 计算回退阈值：必选类型 min 之和的 50%
fn min_cards_threshold(doc_type: DocumentType, char_count: usize) -> usize {
    let plan = CardPlanner::plan(doc_type, char_count);
    plan.iter()
        .filter(|p| p.required)
        .map(|p| p.min)
        .sum::<usize>()
        / 2
}

/// Extract-then-assign：2 次 LLM 调用替代 9 次
/// Step 1: 一次调用提取所有知识点
/// Step 2: 将知识点按类型分配并生成卡片
async fn generate_cards_extract_then_assign(
    document: &str,
    doc_type: DocumentType,
    call_llm: &impl ChatFn,
    load_prompt: &(dyn Fn(&str) -> Result<String> + Send + Sync),
) -> Result<Vec<Card>> {
    let safe_document = sanitize_for_prompt(document);
    let card_plan = CardPlanner::plan_text(doc_type, document.chars().count());

    // Step 1: 提取所有知识点
    let extract_prompt = load_prompt("extract_knowledge")
        .unwrap_or_else(|_| load_prompt("all_cards").unwrap_or_default())
        .replace("{document}", &safe_document);

    let extraction = call_llm
        .call_chat(
            vec![
                LlmMessage {
                    role: "system".to_string(),
                    content: "你是知识提取专家。严格按 JSON 格式输出。".to_string(),
                },
                LlmMessage {
                    role: "user".to_string(),
                    content: extract_prompt,
                },
            ],
            Some(doc_limits_for(document.chars().count()).compile_output as u32),
        )
        .await?;

    // 尝试解析 JSON，失败则尝试从文本中提取
    let knowledge_points = match serde_json::from_str::<serde_json::Value>(&extraction) {
        Ok(json) => json
            .get("knowledge_points")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
        Err(_) => {
            // 尝试从文本中提取知识点（每行一个标题+内容）
            let mut points = Vec::new();
            for line in extraction.lines() {
                let line = line.trim();
                if line.starts_with("- ") || line.starts_with("* ") {
                    points.push(serde_json::json!({"title": line[2..].to_string()}));
                }
            }
            points
        }
    };

    if knowledge_points.is_empty() {
        return Err(crate::error::AppError::TaskPanic(
            "Extract-then-assign: 未提取到知识点".to_string(),
        ));
    }

    // Step 2: 按类型分配并生成卡片
    let assign_prompt = load_prompt("assign_cards")
        .unwrap_or_else(|_| load_prompt("all_cards").unwrap_or_default())
        .replace(
            "{knowledge_points}",
            &serde_json::to_string_pretty(&knowledge_points).unwrap_or_default(),
        )
        .replace("{card_plan}", &card_plan);

    let response = call_llm
        .call_chat(
            vec![
                LlmMessage {
                    role: "system".to_string(),
                    content: "你是卡片笔记分类专家。按格式要求输出每张卡片，用 --- 分隔。"
                        .to_string(),
                },
                LlmMessage {
                    role: "user".to_string(),
                    content: assign_prompt,
                },
            ],
            Some(doc_limits_for(document.chars().count()).compile_output as u32),
        )
        .await?;

    // 解析分配的卡片（从 #卡片类型 标签检测类型）
    let mut cards = parse_assigned_cards(&response)?;

    for card in cards.iter_mut() {
        card.unique_id = uuid::Uuid::now_v7().to_string();
    }

    Ok(cards)
}

/// 旧策略：9 次独立 LLM 调用（保留作为 fallback）
async fn generate_cards_legacy(
    document: &str,
    doc_type: DocumentType,
    call_llm: impl ChatFn,
    load_prompt: &(dyn Fn(&str) -> Result<String> + Send + Sync),
) -> Result<Vec<Card>> {
    let plan = CardPlanner::plan(doc_type, document.chars().count());
    let mut all_cards = Vec::new();
    let safe_document = sanitize_for_prompt(document);

    for item in plan.iter() {
        let prompt_name = card_type_prompt_name(&item.card_type);
        let prompt_template = match load_prompt(prompt_name) {
            Ok(p) => p,
            Err(e) => {
                eprintln!(
                    "    ⚠ 未找到 Prompt '{}': {}, 尝试 fallback",
                    prompt_name, e
                );
                match load_prompt("all_cards") {
                    Ok(fallback) => {
                        eprintln!("    → 使用 all_cards.md 作为 fallback");
                        fallback
                    }
                    Err(e2) => {
                        eprintln!("    ✗ Fallback 也失败: {}", e2);
                        continue;
                    }
                }
            }
        };

        let prompt = prompt_template.replace("{document}", &safe_document);
        let system = format!(
            "你是一位以卡片笔记为信仰的知识炼金术士。当前任务：生成{}。请严格遵循 Prompt 中的格式要求输出。",
            CardPlanner::card_type_name(&item.card_type)
        );

        let response = match call_llm
            .call_chat(
                vec![
                    LlmMessage {
                        role: "system".to_string(),
                        content: system,
                    },
                    LlmMessage {
                        role: "user".to_string(),
                        content: prompt,
                    },
                ],
                Some(doc_limits_for(document.chars().count()).compile_output as u32),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("    ⚠ {} 卡片 LLM 调用失败: {}", prompt_name, e);
                continue;
            }
        };

        let cards = match parse_single_type_cards(&response, item.card_type.clone()) {
            Ok(cards) => cards,
            Err(e) => {
                eprintln!("    ⚠ {} 卡片解析失败: {}", prompt_name, e);
                continue;
            }
        };
        all_cards.extend(cards);
    }

    for card in all_cards.iter_mut() {
        card.unique_id = uuid::Uuid::now_v7().to_string();
    }

    Ok(all_cards)
}

/// 解析 Extract-Then-Assign 输出的卡片，从 #类型 标签自动检测 card_type
fn parse_assigned_cards(response: &str) -> Result<Vec<Card>> {
    let mut cards = Vec::new();
    let card_blocks: Vec<&str> = response
        .split("---")
        .filter(|s| !s.trim().is_empty())
        .collect();

    for block in card_blocks {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }

        // 从 #类型 标签检测卡片类型
        let detected_type = detect_card_type_from_tag(block);

        let title = extract_field(block, "标题").unwrap_or_default();
        let reference = extract_field(block, "ref").unwrap_or_default();
        let original_text = extract_field(block, "原文").unwrap_or_default();
        let source = extract_field(block, "出处").unwrap_or_default();
        let paraphrase = extract_field(block, "仿写").unwrap_or_default();

        let content = build_card_content(block);

        if !title.is_empty() {
            let source = if detected_type == Some(CardType::Quote)
                && source.is_empty()
                && !reference.is_empty()
            {
                reference.clone()
            } else {
                source
            };
            cards.push(Card {
                title,
                content,
                card_type: detected_type.unwrap_or(CardType::Knowledge),
                reference,
                unique_id: String::new(),
                original_text,
                source,
                paraphrase,
                related_cards: Vec::new(),
                source_file: String::new(),
                chunk_id: String::new(),
                evidence: String::new(),
                location: String::new(),
                quality_score: 1.0,
                status: CardStatus::Accepted,
                reject_reason: String::new(),
                retry_count: 0,
                degraded_from: None,
            });
        }
    }

    Ok(cards)
}

/// 从卡片块中检测 #卡片类型 标签
fn detect_card_type_from_tag(block: &str) -> Option<CardType> {
    let first_line = block.lines().next().unwrap_or("").trim();
    match first_line {
        s if s.contains("术语卡") => Some(CardType::Term),
        s if s.contains("新知卡") => Some(CardType::Knowledge),
        s if s.contains("反常识卡") => Some(CardType::CounterIntuit),
        s if s.contains("金句卡") => Some(CardType::Quote),
        s if s.contains("人物卡") => Some(CardType::Person),
        s if s.contains("事件卡") => Some(CardType::Event),
        s if s.contains("行动卡") => Some(CardType::Action),
        s if s.contains("图示卡") => Some(CardType::Graph),
        s if s.contains("新词卡") => Some(CardType::NewWord),
        s if s.contains("综述卡") => Some(CardType::Review),
        s if s.contains("基础卡") | s.contains("笔记卡") => Some(CardType::Note),
        s if s.contains("索引卡") => Some(CardType::Index),
        _ => None,
    }
}

/// 构建卡片内容（排除标题/ref/uuid/标签行）
fn build_card_content(block: &str) -> String {
    let mut lines = Vec::new();
    let mut started = false;

    for line in block.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() && !started {
            continue;
        }

        // 跳过标签行
        if trimmed.starts_with('#') && (trimmed.contains("卡") || trimmed.contains("卡片")) {
            continue;
        }
        // 跳过标题和 ref 字段行
        if (trimmed.starts_with("标题") || trimmed.starts_with("ref"))
            && (trimmed.contains('：') || trimmed.contains(':'))
        {
            continue;
        }
        // 跳过 uuid 行
        if (trimmed.starts_with("uuid") || trimmed.starts_with("唯一编码"))
            && (trimmed.contains('：') || trimmed.contains(':'))
        {
            continue;
        }

        started = true;
        lines.push(line);
    }

    // 去掉尾部空行
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }

    lines.join("\n").trim().to_string()
}

/// 解析单类型卡片响应
fn parse_single_type_cards(response: &str, card_type: CardType) -> Result<Vec<Card>> {
    let mut cards = Vec::new();

    // 按 "---" 分隔符分割卡片
    let card_blocks: Vec<&str> = response
        .split("---")
        .filter(|s| !s.trim().is_empty())
        .collect();

    for block in card_blocks {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }

        let title = extract_field(block, "标题").unwrap_or_default();
        let reference = extract_field(block, "ref").unwrap_or_default();

        // 金句卡专属字段
        let original_text = extract_field(block, "原文").unwrap_or_default();
        let source = extract_field(block, "出处").unwrap_or_default();
        let paraphrase = extract_field(block, "仿写").unwrap_or_default();

        // 构建 content：保留所有类型专属字段，排除通用字段
        let mut content_lines = Vec::new();

        for line in block.lines() {
            let trimmed = line.trim();

            // 跳过开头的空行
            if trimmed.is_empty() && content_lines.is_empty() {
                continue;
            }

            // 跳过标题字段行
            if trimmed.starts_with("标题") && (trimmed.contains("：") || trimmed.contains(":")) {
                continue;
            }

            // 跳过 ref 字段行
            if trimmed.starts_with("ref") && (trimmed.contains("：") || trimmed.contains(":")) {
                continue;
            }

            // 跳过参考字段行（中文）
            if trimmed.starts_with("参考") && (trimmed.contains("：") || trimmed.contains(":")) {
                continue;
            }

            // 跳过 uuid 字段行
            if (trimmed.starts_with("uuid") || trimmed.starts_with("唯一编码"))
                && (trimmed.contains("：") || trimmed.contains(":"))
            {
                continue;
            }

            // 跳过金句卡专属字段行（已单独提取）
            if (trimmed.starts_with("原文")
                || trimmed.starts_with("出处")
                || trimmed.starts_with("仿写"))
                && (trimmed.contains("：") || trimmed.contains(":"))
            {
                continue;
            }

            // 跳过卡片类型标签行
            if trimmed.starts_with("#术语卡")
                || trimmed.starts_with("#新知卡")
                || trimmed.starts_with("#人物卡")
                || trimmed.starts_with("#金句卡")
                || trimmed.starts_with("#事件卡")
                || trimmed.starts_with("#行动卡")
                || trimmed.starts_with("#图示卡")
                || trimmed.starts_with("#新词卡")
                || trimmed.starts_with("#基础卡")
                || trimmed.starts_with("#索引卡")
                || trimmed.starts_with("#反常识卡")
                || trimmed.starts_with("#综述卡")
                || trimmed.starts_with("#技巧卡")
                || trimmed.starts_with("#任意卡")
            {
                continue;
            }

            content_lines.push(line);
        }

        // 去掉末尾的空行
        while let Some(last) = content_lines.last() {
            if last.trim().is_empty() {
                content_lines.pop();
            } else {
                break;
            }
        }

        let content = content_lines.join("\n").trim().to_string();

        if !title.is_empty() {
            // 金句卡：source 用 reference 兜底（prompt 只要求 ref 字段）
            let source =
                if card_type == CardType::Quote && source.is_empty() && !reference.is_empty() {
                    reference.clone()
                } else {
                    source
                };
            cards.push(Card {
                title,
                content,
                card_type: card_type.clone(),
                reference,
                unique_id: String::new(),
                original_text,
                source,
                paraphrase,
                related_cards: Vec::new(),
                source_file: String::new(),
                chunk_id: String::new(),
                evidence: String::new(),
                location: String::new(),
                quality_score: 1.0,
                status: CardStatus::Accepted,
                reject_reason: String::new(),
                retry_count: 0,
                degraded_from: None,
            });
        }
    }

    Ok(cards)
}

/// 预编译字段提取正则，避免每次调用都编译
static RE_EXTRACT_FIELD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^(.+?)[:：]\s*(.+?)$").expect("hardcoded regex is valid"));

/// 从文本块中提取字段
fn extract_field(block: &str, field_name: &str) -> Option<String> {
    for cap in RE_EXTRACT_FIELD.captures_iter(block) {
        let name = cap.get(1)?.as_str().trim();
        if name == field_name {
            return Some(cap.get(2)?.as_str().trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_card_type_prompt_name() {
        assert_eq!(
            card_type_prompt_name(&CardType::Knowledge),
            "knowledge_card"
        );
        assert_eq!(card_type_prompt_name(&CardType::Term), "term_card");
        assert_eq!(
            card_type_prompt_name(&CardType::CounterIntuit),
            "counter_intuit_card"
        );
        assert_eq!(card_type_prompt_name(&CardType::Review), "review_card");
    }

    #[test]
    fn test_parse_single_type_cards_term() {
        let response = "---\n标题：执行意图\n定义：它是一种制订计划的方式。\n解释：通过在大脑中提前规划执行计划的时间、地点，从而更易引发行动。\n例子：你可以将「我要多运动」改写为「如果到了每天傍晚5点，那么我就去操场跑步」。\nref：人生模式_p160\nuuid：202001011942\n#术语卡\n---";
        let cards = parse_single_type_cards(response, CardType::Term).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].title, "执行意图");
        assert_eq!(cards[0].card_type, CardType::Term);
        assert_eq!(cards[0].reference, "人生模式_p160");
        // content 应该包含定义、解释、例子，但不包含标题、ref、uuid、标签
        assert!(cards[0].content.contains("定义："));
        assert!(cards[0].content.contains("解释："));
        assert!(cards[0].content.contains("例子："));
        assert!(!cards[0].content.contains("ref："));
        assert!(!cards[0].content.contains("uuid："));
    }

    #[test]
    fn test_parse_single_type_cards_knowledge() {
        let response = "---\n标题：阅读方法不是通用的\n已知：阅读一本书就是从头到尾逐页逐段阅读\n新知：不同类型的书需要用不同的阅读次序和技法\n例子：学术专著需要结构阅读→抽样阅读→文本细读→主题阅读\nref：阳志平《聪明的阅读者》\nuuid：202305021641\n#新知卡\n---";
        let cards = parse_single_type_cards(response, CardType::Knowledge).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].title, "阅读方法不是通用的");
        assert_eq!(cards[0].card_type, CardType::Knowledge);
        assert!(cards[0].content.contains("已知："));
        assert!(cards[0].content.contains("新知："));
    }

    #[test]
    fn test_parse_single_type_cards_multiple() {
        let response = "---\n标题：卡片A\n定义：定义A\nref：来源A\n#术语卡\n---\n---\n标题：卡片B\n定义：定义B\nref：来源B\n#术语卡\n---";
        let cards = parse_single_type_cards(response, CardType::Term).unwrap();
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].title, "卡片A");
        assert_eq!(cards[1].title, "卡片B");
    }

    #[test]
    fn test_extract_field_found() {
        let block = "标题：字段值A\n内容：字段值B\n";
        assert_eq!(extract_field(block, "标题"), Some("字段值A".to_string()));
        assert_eq!(extract_field(block, "内容"), Some("字段值B".to_string()));
    }

    #[test]
    fn test_extract_field_not_found() {
        let block = "标题：字段值C\n";
        assert_eq!(extract_field(block, "不存在的字段"), None);
    }

    #[test]
    fn test_extract_field_colon_variant() {
        let block = "标题: 值A\n内容：值B\n";
        assert_eq!(extract_field(block, "标题"), Some("值A".to_string()));
        assert_eq!(extract_field(block, "内容"), Some("值B".to_string()));
    }
}
