use serde::{Deserialize, Serialize};
use std::fmt;

fn default_card_quality_score() -> f64 {
    1.0
}

/// 卡片类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum CardType {
    #[default]
    Knowledge, // 新知卡（默认）
    Person,        // 人物卡
    Term,          // 术语卡
    Quote,         // 金句卡
    Event,         // 事件卡
    Action,        // 行动卡
    Graph,         // 图示卡
    NewWord,       // 新词卡
    Note,          // 基础卡
    Index,         // 索引卡
    CounterIntuit, // 反常识卡
    Review,        // 综述卡
}

impl CardType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CardType::Person => "人物卡",
            CardType::Term => "术语卡",
            CardType::Knowledge => "新知卡",
            CardType::Quote => "金句卡",
            CardType::Event => "事件卡",
            CardType::Action => "行动卡",
            CardType::Graph => "图示卡",
            CardType::NewWord => "新词卡",
            CardType::Note => "基础卡",
            CardType::Index => "索引卡",
            CardType::CounterIntuit => "反常识卡",
            CardType::Review => "综述卡",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "人物卡" => Some(CardType::Person),
            "术语卡" => Some(CardType::Term),
            "新知卡" => Some(CardType::Knowledge),
            "金句卡" => Some(CardType::Quote),
            "事件卡" => Some(CardType::Event),
            "行动卡" => Some(CardType::Action),
            "图示卡" => Some(CardType::Graph),
            "新词卡" => Some(CardType::NewWord),
            "基础卡" => Some(CardType::Note),
            "索引卡" => Some(CardType::Index),
            "反常识卡" => Some(CardType::CounterIntuit),
            "综述卡" => Some(CardType::Review),
            _ => None,
        }
    }
}

impl fmt::Display for CardType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 卡片质量状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CardStatus {
    #[default]
    Accepted,
    NeedsRetry,
    Degraded,
    Rejected,
}

/// 卡片模型
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Card {
    pub title: String,
    pub content: String,
    pub card_type: CardType,
    #[serde(default)]
    pub reference: String,
    #[serde(default)]
    pub unique_id: String,
    // 金句卡扩展字段
    #[serde(default)]
    pub original_text: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub paraphrase: String,
    #[serde(default)]
    pub related_cards: Vec<String>,
    #[serde(default)]
    pub source_file: String,
    #[serde(default)]
    pub chunk_id: String,
    #[serde(default)]
    pub evidence: String,
    #[serde(default)]
    pub location: String,
    #[serde(default = "default_card_quality_score")]
    pub quality_score: f64,
    #[serde(default)]
    pub status: CardStatus,
    #[serde(default)]
    pub reject_reason: String,
    #[serde(default)]
    pub retry_count: u8,
    #[serde(default)]
    pub degraded_from: Option<CardType>,
}

impl Card {
    pub fn to_markdown(&self) -> String {
        match self.card_type {
            CardType::Quote => self.to_quote_markdown(),
            _ => self.to_default_markdown(),
        }
    }

    fn to_default_markdown(&self) -> String {
        let mut lines = vec![
            format!("# {}", self.card_type),
            "".to_string(),
            format!("**标题：** {}", self.title),
            "".to_string(),
            self.content.clone(),
            "".to_string(),
        ];
        if !self.reference.is_empty() {
            lines.push(format!("**ref：** {}", self.reference));
            lines.push("".to_string());
        }
        self.append_traceability_markdown(&mut lines);
        lines.push(format!("**uuid：** {}", self.unique_id));
        lines.push("".to_string());
        lines.push("***".to_string());
        lines.push("".to_string());
        lines.join("\n")
    }

    fn append_traceability_markdown(&self, lines: &mut Vec<String>) {
        if !self.source_file.is_empty() {
            lines.push(format!("**来源文件：** {}", self.source_file));
            lines.push("".to_string());
        }
        if !self.chunk_id.is_empty() || !self.location.is_empty() {
            let mut parts = Vec::new();
            if !self.chunk_id.is_empty() {
                parts.push(format!("chunk {}", self.chunk_id));
            }
            if !self.location.is_empty() {
                parts.push(self.location.clone());
            }
            lines.push(format!("**位置：** {}", parts.join(" / ")));
            lines.push("".to_string());
        }
        if !self.evidence.is_empty() {
            lines.push(format!("**证据：** {}", self.evidence));
            lines.push("".to_string());
        }
        if self.quality_score < 1.0 || self.status != CardStatus::Accepted {
            lines.push(format!(
                "**质量：** {:.0}% / {:?}",
                self.quality_score * 100.0,
                self.status
            ));
            lines.push("".to_string());
        }
        if !self.reject_reason.is_empty() {
            lines.push(format!("**处理原因：** {}", self.reject_reason));
            lines.push("".to_string());
        }
    }

    /// 应用中文排版修复，返回修复数量
    pub fn typo_fix(&mut self) -> usize {
        use crate::quality::typo_lint;
        let mut count = 0;

        let title_result = typo_lint::typo_lint(&self.title);
        count += title_result.issues.len();
        self.title = title_result.fixed_text;

        let content_result = typo_lint::typo_lint(&self.content);
        count += content_result.issues.len();
        self.content = content_result.fixed_text;

        if !self.original_text.is_empty() {
            let result = typo_lint::typo_lint(&self.original_text);
            count += result.issues.len();
            self.original_text = result.fixed_text;
        }
        if !self.source.is_empty() {
            let result = typo_lint::typo_lint(&self.source);
            count += result.issues.len();
            self.source = result.fixed_text;
        }
        if !self.paraphrase.is_empty() {
            let result = typo_lint::typo_lint(&self.paraphrase);
            count += result.issues.len();
            self.paraphrase = result.fixed_text;
        }

        count
    }

    fn to_quote_markdown(&self) -> String {
        let mut lines = vec![
            format!("# {}", self.card_type),
            "".to_string(),
            format!("**标题：** {}", self.title),
            "".to_string(),
        ];
        if !self.original_text.is_empty() {
            lines.push(format!("**原文：** {}", self.original_text));
            lines.push("".to_string());
        }
        if !self.source.is_empty() {
            lines.push(format!("**出处：** {}", self.source));
            lines.push("".to_string());
        }
        if !self.paraphrase.is_empty() {
            lines.push(format!("**仿写：** {}", self.paraphrase));
            lines.push("".to_string());
        }
        if !self.content.is_empty() && self.content != self.original_text {
            lines.push(self.content.clone());
            lines.push("".to_string());
        }
        for related in &self.related_cards {
            lines.push(format!("> 关联卡片：「{}」", related));
        }
        if !self.related_cards.is_empty() {
            lines.push("".to_string());
        }
        if !self.reference.is_empty() {
            lines.push(format!("**ref：** {}", self.reference));
            lines.push("".to_string());
        }
        self.append_traceability_markdown(&mut lines);
        lines.push(format!("**uuid：** {}", self.unique_id));
        lines.push("".to_string());
        lines.push("***".to_string());
        lines.push("".to_string());
        lines.join("\n")
    }
}

/// 实体模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub name: String,
    pub entity_type: String,
    #[serde(default)]
    pub context: String,
}

/// 关系模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub source: String,
    pub target: String,
    pub relation_type: String,
    #[serde(default)]
    pub evidence: String,
}

/// 知识图谱模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraph {
    pub entities: Vec<Entity>,
    pub relations: Vec<Relation>,
}

/// Mermaid 安全转义：处理会破坏 Mermaid 语法的特殊字符
fn mermaid_escape(text: &str) -> String {
    text.replace('"', "'") // 双引号 → 单引号（避免中断 Mermaid 字符串）
        .replace('#', "\\#") // 井号 → 转义（避免被误认为注释）
        .replace('\r', " ")
        .replace('\n', " ")
        .replace('\t', " ")
}

impl KnowledgeGraph {
    pub fn to_mermaid(&self) -> String {
        let mut lines = vec!["graph TD".to_string()];
        let mut node_map: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut node_counter = 0;

        // 构建节点ID映射：name -> n0, n1, ...
        let mut all_names = std::collections::HashSet::new();
        for rel in &self.relations {
            all_names.insert(rel.source.clone());
            all_names.insert(rel.target.clone());
        }
        for name in all_names {
            node_map.insert(name, format!("n{}", node_counter));
            node_counter += 1;
        }

        // 输出节点定义（带标签）
        for (name, id) in &node_map {
            let safe_label = mermaid_escape(name);
            lines.push(format!("    {}[\"{}\"]", id, safe_label));
        }

        // 输出关系
        for rel in &self.relations {
            let source_id = node_map.get(&rel.source).unwrap_or(&rel.source);
            let target_id = node_map.get(&rel.target).unwrap_or(&rel.target);
            let safe_type = mermaid_escape(&rel.relation_type);
            lines.push(format!(
                "    {} -- \"{}\" --> {}",
                source_id, safe_type, target_id
            ));
        }
        lines.join("\n")
    }
}

/// 摘要模型
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Summary {
    pub title: String,
    pub overview: String,
    pub key_points: Vec<String>,
    #[serde(default)]
    pub structure: String,
}

impl Summary {
    pub fn to_markdown(&self) -> String {
        let mut lines = vec![
            format!("# {} — 核心摘要", self.title),
            "".to_string(),
            "## 概述".to_string(),
            "".to_string(),
            self.overview.clone(),
            "".to_string(),
            "## 核心要点".to_string(),
            "".to_string(),
        ];
        for (i, point) in self.key_points.iter().enumerate() {
            lines.push(format!("{}. {}", i + 1, point));
        }
        lines.push("".to_string());
        lines.push("## 结构".to_string());
        lines.push("".to_string());
        lines.push(self.structure.clone());
        lines.push("".to_string());
        lines.join("\n")
    }
}

/// 单篇文档模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    #[serde(default)]
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub source_file: String,
}

/// 章节规划
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterPlan {
    pub title: String,
    #[serde(default)]
    pub focus: String,
    #[serde(default)]
    pub advance_point: String,
    #[serde(default)]
    pub narrative_outline: String,
    #[serde(default)]
    pub source_documents: Vec<String>,
}

/// 分块信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkInfo {
    pub title_path: String,
    pub size: usize,
    pub entities: usize,
    pub cards: usize,
    pub relations: usize,
}

/// 编译阶段诊断（记录失败、降级、重试等信息）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompilationDiagnostics {
    /// 失败的阶段及错误信息
    #[serde(default)]
    pub failures: Vec<StageFail>,
    /// 被降级处理的阶段（返回默认值或部分结果）
    #[serde(default)]
    pub degradations: Vec<StageDegradation>,
    /// 重试成功的阶段
    #[serde(default)]
    pub retries: Vec<StageRetry>,
}

/// 单个阶段的失败信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageFail {
    pub stage: String, // "summary" / "entities" / "cards" / "graph"
    pub error: String,
    pub retry_count: u8,
    pub final_status: String, // "failed" / "skipped" / "partially_recovered"
}

/// 单个阶段的降级信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageDegradation {
    pub stage: String,
    pub reason: String,
    pub expected_count: usize, // 预期生成数量
    pub actual_count: usize,   // 实际生成数量
}

/// 单个阶段的重试成功
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageRetry {
    pub stage: String,
    pub retry_count: u8,
    pub success: bool,
}

/// 单篇文档编译结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationResult {
    pub source_file: String,
    pub summary: Summary,
    pub cards: Vec<Card>,
    pub graph: KnowledgeGraph,
    #[serde(default)]
    pub chunks: Vec<ChunkInfo>,
    /// 编译过程中的诊断信息
    #[serde(default)]
    pub diagnostics: CompilationDiagnostics,
}

/// 书籍级编译结果
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BookCompilationResult {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub chapter_plans: Vec<ChapterPlan>,
    #[serde(default)]
    pub all_cards: Vec<Card>,
    #[serde(default)]
    pub all_entities: Vec<Entity>,
    #[serde(default)]
    pub all_relations: Vec<Relation>,
    #[serde(default)]
    pub cross_document_relations: Vec<Relation>,
}

/// LLM 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: String,
    pub content: String,
}

/// LLM 请求
#[derive(Debug, Clone, Serialize)]
pub struct LlmRequest {
    pub model: String,
    pub messages: Vec<LlmMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
}

/// LLM 调用用量统计
#[derive(Debug, Clone, Default)]
pub struct LlmUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    /// 缓存命中 token 数（Anthropic cache_read_input_tokens）
    pub cached_tokens: u32,
    /// 缓存创建 token 数（Anthropic cache_creation_input_tokens）
    pub cache_creation_tokens: u32,
    pub model: String,
    pub latency_ms: u64,
}

impl LlmUsage {
    pub fn format_report(usages: &[Self]) -> String {
        if usages.is_empty() {
            return "无 LLM 调用记录".to_string();
        }
        let total_prompt: u32 = usages.iter().map(|u| u.prompt_tokens).sum();
        let total_completion: u32 = usages.iter().map(|u| u.completion_tokens).sum();
        let total: u32 = usages.iter().map(|u| u.total_tokens).sum();
        let total_cached: u32 = usages.iter().map(|u| u.cached_tokens).sum();
        let avg_latency: u64 = usages.iter().map(|u| u.latency_ms).sum::<u64>() / usages.len() as u64;
        let mut lines = vec![
            "╔════════════════════════════════════════════════════════════╗".to_string(),
            "║                 LLM 调用用量报告                          ║".to_string(),
            "╠════════════════════════════════════════════════════════════╣".to_string(),
            format!("║ 调用次数: {:<45}║", usages.len()),
            format!("║ Input tokens:  {:<39}║", total_prompt),
            format!("║ Output tokens: {:<39}║", total_completion),
            format!("║ Total tokens:  {:<39}║", total),
        ];
        if total_cached > 0 {
            lines.push(format!("║ Cached tokens: {:<39}║", total_cached));
        }
        lines.push(format!("║ 平均延迟: {:<44}║", format!("{}ms", avg_latency)));
        lines.push("╚════════════════════════════════════════════════════════════╝".to_string());
        lines.join("\n")
    }
}

/// 响应格式(用于 JSON 模式)
#[derive(Debug, Clone, Serialize)]
pub struct ResponseFormat {
    #[serde(rename = "type")]
    pub format_type: String,
}

impl ResponseFormat {
    pub fn json_object() -> Self {
        Self {
            format_type: "json_object".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_card_type_parse() {
        assert_eq!(CardType::parse("人物卡"), Some(CardType::Person));
        assert_eq!(CardType::parse("术语卡"), Some(CardType::Term));
        assert_eq!(CardType::parse("新知卡"), Some(CardType::Knowledge));
        assert_eq!(CardType::parse("金句卡"), Some(CardType::Quote));
        assert_eq!(CardType::parse("事件卡"), Some(CardType::Event));
        assert_eq!(CardType::parse("行动卡"), Some(CardType::Action));
        assert_eq!(CardType::parse("图示卡"), Some(CardType::Graph));
        assert_eq!(CardType::parse("新词卡"), Some(CardType::NewWord));
        assert_eq!(CardType::parse("基础卡"), Some(CardType::Note));
        assert_eq!(CardType::parse("索引卡"), Some(CardType::Index));
        assert_eq!(CardType::parse("未知类型"), None);
    }

    #[test]
    fn test_card_type_display() {
        assert_eq!(format!("{}", CardType::Person), "人物卡");
        assert_eq!(format!("{}", CardType::Quote), "金句卡");
    }

    #[test]
    fn test_card_to_markdown_default() {
        let card = Card {
            title: "标题A".to_string(),
            content: "正文内容A".to_string(),
            card_type: CardType::Knowledge,
            reference: "来源A".to_string(),
            unique_id: "20240101120000".to_string(),
            original_text: "".to_string(),
            source: "".to_string(),
            paraphrase: "".to_string(),
            related_cards: vec![],
            ..Default::default()
        };
        let md = card.to_markdown();
        assert!(md.contains("**标题：** 标题A"));
        assert!(md.contains("正文内容A"));
        assert!(md.contains("**ref：** 来源A"));
        assert!(md.contains("**uuid：** 20240101120000"));
        assert!(md.contains("# 新知卡"));
    }

    #[test]
    fn test_card_to_markdown_quote() {
        let card = Card {
            title: "标题B".to_string(),
            content: "正文内容B".to_string(),
            card_type: CardType::Quote,
            reference: "".to_string(),
            unique_id: "20240101120001".to_string(),
            original_text: "原文A".to_string(),
            source: "出处A".to_string(),
            paraphrase: "仿写A".to_string(),
            related_cards: vec!["关联A".to_string()],
            ..Default::default()
        };
        let md = card.to_markdown();
        assert!(md.contains("**原文：** 原文A"));
        assert!(md.contains("**出处：** 出处A"));
        assert!(md.contains("**仿写：** 仿写A"));
        assert!(md.contains("关联A"));
    }

    #[test]
    fn test_knowledge_graph_to_mermaid() {
        let graph = KnowledgeGraph {
            entities: vec![],
            relations: vec![
                Relation {
                    source: "A".to_string(),
                    target: "B".to_string(),
                    relation_type: "关系A".to_string(),
                    evidence: "".to_string(),
                },
                Relation {
                    source: "C".to_string(),
                    target: "D".to_string(),
                    relation_type: "值A\"".to_string(),
                    evidence: "".to_string(),
                },
            ],
        };
        let mermaid = graph.to_mermaid();
        assert!(mermaid.starts_with("graph TD"));
        // 新格式使用安全的节点ID (n0, n1, ...) 而不是直接使用实体名称
        assert!(mermaid.contains("-- \"关系A\" -->"));
        assert!(mermaid.contains("-- \"值A'\" -->")); // 双引号被替换为单引号
        // 验证节点标签包含中文实体名称（无多余换行）
        assert!(mermaid.contains("[\"A\"]"));
        assert!(mermaid.contains("[\"B\"]"));
        assert!(mermaid.contains("[\"C\"]"));
        assert!(mermaid.contains("[\"D\"]"));
    }

    #[test]
    fn test_summary_to_markdown() {
        let summary = Summary {
            title: "标题C".to_string(),
            overview: "概述A".to_string(),
            key_points: vec!["要点A".to_string(), "要点B".to_string()],
            structure: "结构A\n结构B".to_string(),
        };
        let md = summary.to_markdown();
        assert!(md.contains("# 标题C — 核心摘要"));
        assert!(md.contains("## 概述"));
        assert!(md.contains("概述A"));
        assert!(md.contains("1. 要点A"));
        assert!(md.contains("2. 要点B"));
        assert!(md.contains("## 结构"));
        assert!(md.contains("结构A"));
    }

    // ── Mermaid 安全转义边界测试 ──

    #[test]
    fn test_mermaid_escape_double_quote() {
        assert_eq!(mermaid_escape("a\"b"), "a'b");
    }

    #[test]
    fn test_mermaid_escape_hash() {
        assert_eq!(mermaid_escape("#新知卡"), "\\#新知卡");
    }

    #[test]
    fn test_mermaid_escape_newline() {
        assert_eq!(mermaid_escape("第一行\n第二行"), "第一行 第二行");
    }

    #[test]
    fn test_mermaid_escape_crlf() {
        assert_eq!(mermaid_escape("A\r\nB"), "A  B");
    }

    #[test]
    fn test_mermaid_escape_tab() {
        assert_eq!(mermaid_escape("A\tB"), "A B");
    }

    #[test]
    fn test_mermaid_escape_combined() {
        let input = "#卡片\"名\"\n含换行";
        assert_eq!(mermaid_escape(input), "\\#卡片'名' 含换行");
    }

    #[test]
    fn test_mermaid_graph_with_hash_in_node() {
        let graph = KnowledgeGraph {
            entities: vec![],
            relations: vec![Relation {
                source: "#新知卡".to_string(),
                target: "B".to_string(),
                relation_type: "包含".to_string(),
                evidence: "".to_string(),
            }],
        };
        let mermaid = graph.to_mermaid();
        // 节点标签中的 # 应被转义为 \#
        assert!(mermaid.contains("[\"\\#新知卡\"]"));
        // 不应出现未转义的 #（会被 Mermaid 解析为注释）
        assert!(!mermaid.contains("[#新知卡"));
    }

    #[test]
    fn test_mermaid_graph_with_newline_in_relation() {
        let graph = KnowledgeGraph {
            entities: vec![],
            relations: vec![Relation {
                source: "A".to_string(),
                target: "B".to_string(),
                relation_type: "关系\n含换行".to_string(),
                evidence: "".to_string(),
            }],
        };
        let mermaid = graph.to_mermaid();
        // 关系文本中的换行应被替换为空格
        assert!(mermaid.contains("-- \"关系 含换行\" -->"));
        // 不应出现原始换行符在关系文本中
        assert!(!mermaid.contains("-- \"关系\n含换行\" -->"));
    }

    #[test]
    fn test_mermaid_graph_empty() {
        let graph = KnowledgeGraph {
            entities: vec![],
            relations: vec![],
        };
        let mermaid = graph.to_mermaid();
        assert_eq!(mermaid, "graph TD");
    }
}
