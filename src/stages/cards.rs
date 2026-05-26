use std::sync::{LazyLock, Mutex};

use chrono::Local;

use crate::config::DOC_LIMITS;
use crate::doc_type::DocumentType;
use crate::error::Result;
use crate::models::{Card, CardStatus, CardType, LlmMessage};
use crate::stages::common::ChatFn;

/// 全局状态：记录最后分配的秒级 Unix 时间戳，确保跨批次 unique_id 不重复
static LAST_UNIQUE_SEC: LazyLock<Mutex<i64>> = LazyLock::new(|| Mutex::new(0));

/// 唯一编码格式：YYYYMMDDHHMMSS（14 位）
pub const UNIQUE_ID_FORMAT: &str = "%Y%m%d%H%M%S";

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

    fn card_type_name(card_type: &CardType) -> &'static str {
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
        let scale = if char_count > 50000 { 2 } else { 1 };
        vec![
            CardPlanItem::new(CardType::Knowledge, 3 * scale, 5 * scale, true, 1),
            CardPlanItem::new(CardType::Term, 2 * scale, 4 * scale, true, 2),
            CardPlanItem::new(CardType::Review, 1 * scale, 2 * scale, true, 3),
            CardPlanItem::new(CardType::CounterIntuit, 1 * scale, 3 * scale, true, 4),
            CardPlanItem::new(CardType::Action, 1, 3, false, 5),
            CardPlanItem::new(CardType::Person, 0, 2, false, 6),
            CardPlanItem::new(CardType::Quote, 0, 3, false, 7),
            CardPlanItem::new(CardType::Event, 0, 2, false, 8),
            CardPlanItem::new(CardType::Index, 0, 2, false, 9),
        ]
    }

    /// 论文规划：学术性强，优先术语卡、新知卡、反常识卡、综述卡
    fn plan_paper(char_count: usize) -> Vec<CardPlanItem> {
        let scale = if char_count > 30000 { 2 } else { 1 };
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

/// 生成所有类型的卡片（一次 API 调用）
pub async fn generate_cards(
    document: &str,
    doc_type: DocumentType,
    call_llm: impl ChatFn,
    load_prompt: &(dyn Fn(&str) -> Result<String> + Send + Sync),
) -> Result<Vec<Card>> {
    let prompt_template = load_prompt("all_cards")?;
    // 生成卡片规划文本
    let plan_text = CardPlanner::plan_text(doc_type, document.len());
    let prompt = prompt_template
        .replace("{card_plan}", &plan_text)
        .replace("{document}", document);

    let system = "你是一位知识卡片专家，擅长将文档内容转化为多种类型的高质量知识卡片。".to_string();

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
            Some(DOC_LIMITS.compile_output as u32),
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("    ⚠ 卡片 LLM 调用失败，返回空列表: {}", e);
            return Ok(Vec::new());
        }
    };

    let mut all_cards = match parse_all_cards(&response) {
        Ok(cards) => cards,
        Err(e) => {
            eprintln!("    ⚠ 卡片解析失败，返回空列表: {}", e);
            return Ok(Vec::new());
        }
    };

    // 唯一编码：YYYYMMDDHHMMSS（14位格式不变）
    // 通过全局状态确保跨批次不重复：若上次生成占用到 T，本次从 T+1 开始
    let base = Local::now();
    let base_sec = base.timestamp();
    let mut last_sec = LAST_UNIQUE_SEC.lock().unwrap();
    let start_sec = std::cmp::max(base_sec, *last_sec + 1);

    for (i, card) in all_cards.iter_mut().enumerate() {
        let ts = base + chrono::Duration::seconds((start_sec - base_sec) as i64 + i as i64);
        card.unique_id = ts.format(UNIQUE_ID_FORMAT).to_string();
    }

    if !all_cards.is_empty() {
        *last_sec = start_sec + all_cards.len() as i64 - 1;
    }

    Ok(all_cards)
}

/// 解析统一卡片响应（包含多种类型）
fn parse_all_cards(response: &str) -> Result<Vec<Card>> {
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

        // 从 "类型：" 字段识别卡片类型
        let card_type = parse_card_type(block);

        let mut card = Card {
            title: extract_field(block, "标题").unwrap_or_default(),
            content: extract_field(block, "内容")
                .or_else(|| extract_content(block))
                .unwrap_or_default(),
            card_type,
            reference: extract_field(block, "参考").unwrap_or_default(),
            unique_id: String::new(),
            original_text: extract_field(block, "原文").unwrap_or_default(),
            source: extract_field(block, "出处").unwrap_or_default(),
            paraphrase: extract_field(block, "仿写").unwrap_or_default(),
            related_cards: Vec::new(),
            source_file: extract_field(block, "来源文件").unwrap_or_default(),
            chunk_id: extract_field(block, "chunk").unwrap_or_default(),
            evidence: extract_field(block, "证据").unwrap_or_default(),
            location: extract_field(block, "位置").unwrap_or_default(),
            quality_score: 1.0,
            status: CardStatus::Accepted,
            reject_reason: String::new(),
            retry_count: 0,
            degraded_from: None,
        };

        // 提取关联卡片
        if let Some(related) = extract_field(block, "关联卡片") {
            card.related_cards = related
                .split('、')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }

        if !card.title.is_empty() {
            cards.push(card);
        }
    }

    Ok(cards)
}

/// 从文本块中解析卡片类型
fn parse_card_type(block: &str) -> CardType {
    let type_str = extract_field(block, "类型").unwrap_or_default();
    match type_str.as_str() {
        "人物卡" | "人物" => CardType::Person,
        "术语卡" | "术语" => CardType::Term,
        "新知卡" | "新知" | "知识卡" => CardType::Knowledge,
        "金句卡" | "金句" | "引用卡" => CardType::Quote,
        "事件卡" | "事件" => CardType::Event,
        "行动卡" | "行动" | "方法卡" => CardType::Action,
        "图示卡" | "图示" | "图谱卡" => CardType::Graph,
        "生字卡" | "生字" | "词汇卡" => CardType::NewWord,
        "笔记卡" | "笔记" => CardType::Note,
        "目录卡" | "目录" | "索引卡" => CardType::Index,
        "反常识卡" | "反常识" | "洞见卡" => CardType::CounterIntuit,
        "综述卡" | "综述" | "总结卡" => CardType::Review,
        _ => CardType::Knowledge,
    }
}

/// 从文本块中提取字段
fn extract_field(block: &str, field_name: &str) -> Option<String> {
    let pattern = format!("{}[:：]\\s*(.+?)(?:\\n|$)", regex::escape(field_name));
    let re = regex::Regex::new(&pattern).ok()?;
    re.captures(block)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
}

/// 提取卡片正文内容（标题和参考之间的内容）
fn extract_content(block: &str) -> Option<String> {
    let lines: Vec<&str> = block.lines().collect();
    let mut content_lines = Vec::new();
    let mut in_content = false;

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.starts_with("标题") || trimmed.starts_with("# ") {
            in_content = true;
            continue;
        }
        if trimmed.starts_with("参考")
            || trimmed.starts_with("唯一编码")
            || trimmed.starts_with("#")
        {
            break;
        }
        if in_content && !trimmed.is_empty() {
            content_lines.push(*line);
        }
    }

    if content_lines.is_empty() {
        // 兜底：返回整个块除第一行外的内容
        if lines.len() > 1 {
            return Some(lines[1..].join("\n").trim().to_string());
        }
        return None;
    }

    Some(content_lines.join("\n").trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_all_cards_single() {
        let response = "---\n类型：新知卡\n标题：标题A\n内容：正文内容A。\n参考：来源A\n---";
        let cards = parse_all_cards(response).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].title, "标题A");
        assert_eq!(cards[0].card_type, CardType::Knowledge);
        assert_eq!(cards[0].reference, "来源A");
    }

    #[test]
    fn test_parse_all_cards_multiple_types() {
        let response = "---\n类型：术语卡\n标题：标题B\n内容：正文内容B\n---\n---\n类型：人物卡\n标题：标题C\n内容：正文内容C\n---";
        let cards = parse_all_cards(response).unwrap();
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].title, "标题B");
        assert_eq!(cards[0].card_type, CardType::Term);
        assert_eq!(cards[1].title, "标题C");
        assert_eq!(cards[1].card_type, CardType::Person);
    }

    #[test]
    fn test_parse_all_cards_quote_type() {
        let response = "---\n类型：金句卡\n标题：标题D\n原文：原文A\n出处：出处A\n仿写：仿写A\n---";
        let cards = parse_all_cards(response).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].card_type, CardType::Quote);
        assert_eq!(cards[0].original_text, "原文A");
        assert_eq!(cards[0].source, "出处A");
        assert_eq!(cards[0].paraphrase, "仿写A");
    }

    #[test]
    fn test_parse_all_cards_related_cards() {
        let response = "---\n类型：新知卡\n标题：标题E\n内容\n关联卡片：关联A、关联B\n---";
        let cards = parse_all_cards(response).unwrap();
        assert_eq!(cards[0].related_cards.len(), 2);
        assert_eq!(cards[0].related_cards[0], "关联A");
        assert_eq!(cards[0].related_cards[1], "关联B");
    }

    #[test]
    fn test_parse_all_cards_empty_title_skipped() {
        let response =
            "---\n类型：新知卡\n标题：\n\n---\n---\n类型：笔记卡\n标题：标题F\n内容\n---";
        let cards = parse_all_cards(response).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].title, "标题F");
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

    #[test]
    fn test_extract_content_basic() {
        let block = "标题：标题G\n\n正文第一行\n正文第二行\n\n参考：来源B\n";
        let content = extract_content(block);
        assert_eq!(content, Some("正文第一行\n正文第二行".to_string()));
    }

    #[test]
    fn test_extract_content_fallback() {
        let block = "标题：标题H\n\n正文内容\n";
        let content = extract_content(block);
        assert!(content.is_some());
    }

    #[test]
    fn test_extract_content_empty() {
        let block = "标题：标题H";
        let content = extract_content(block);
        assert_eq!(content, None);
    }

    // ── 边界测试 ──

    #[test]
    fn test_parse_all_cards_empty_json() {
        let json = serde_json::json!({});
        let result = parse_all_cards(&json.to_string());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_all_cards_empty_array() {
        let json = serde_json::json!({ "cards": [] });
        let result = parse_all_cards(&json.to_string());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_all_cards_missing_fields() {
        // parse_all_cards 解析 markdown 格式文本块，不是 JSON
        let text = r#"标题：标题A
类型：术语卡
---
类型：术语卡
内容：没有标题的块
"#;
        let result = parse_all_cards(text);
        // 有标题的保留，没有标题的被跳过
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_extract_field_various_separators() {
        assert_eq!(extract_field("键：值", "键"), Some("值".to_string()));
        assert_eq!(extract_field("键: 值", "键"), Some("值".to_string()));
        assert_eq!(extract_field("键:值", "键"), Some("值".to_string()));
        assert_eq!(extract_field("键 值", "键"), None);
    }

    #[test]
    fn test_extract_content_no_marker() {
        let block = "没有标记的普通文本";
        let content = extract_content(block);
        assert_eq!(content, None);
    }
}
