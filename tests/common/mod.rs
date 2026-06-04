//! 共享测试基础设施 — mock LLM、测试数据工厂、辅助函数
//!
//! 本模块为所有集成测试和 E2E 测试提供统一的 mock 工具。

#![allow(dead_code)]

use std::sync::Mutex;

use cardnote_compiler::error::Result;
use cardnote_compiler::models::{Card, CardType, Entity, LlmMessage, Relation, Summary};
use cardnote_compiler::stages::common::{ChatFn, ChatJsonFn};
use serde_json::Value;

// ═══════════════════════════════════════════════════════
//  Mock ChatFn 实现
// ═══════════════════════════════════════════════════════

/// 可预设响应的 Mock Chat 实现（用于文本阶段：summary）
pub struct MockChat {
    response: Mutex<String>,
}

impl MockChat {
    pub fn new(response: impl Into<String>) -> Self {
        Self {
            response: Mutex::new(response.into()),
        }
    }
}

impl ChatFn for MockChat {
    async fn call_chat(
        &self,
        _messages: Vec<LlmMessage>,
        _max_tokens: Option<u32>,
    ) -> Result<String> {
        Ok(self.response.lock().unwrap().clone())
    }
}

// ═══════════════════════════════════════════════════════
//  Mock ChatJsonFn 实现
// ═══════════════════════════════════════════════════════

/// 可预设响应的 Mock JSON Chat 实现（用于 JSON 阶段：entities/cards/graph）
pub struct MockChatJson {
    response: Mutex<Value>,
}

impl MockChatJson {
    pub fn new(response: Value) -> Self {
        Self {
            response: Mutex::new(response),
        }
    }

    pub fn from_json_str(json_str: &str) -> Self {
        let value = serde_json::from_str(json_str).unwrap_or(Value::Null);
        Self::new(value)
    }
}

impl ChatJsonFn for MockChatJson {
    async fn call_json(
        &self,
        _messages: Vec<LlmMessage>,
        _max_tokens: Option<u32>,
    ) -> Result<Value> {
        Ok(self.response.lock().unwrap().clone())
    }
}

// ═══════════════════════════════════════════════════════
//  组合 Mock：同时实现 ChatFn + ChatJsonFn
// ═══════════════════════════════════════════════════════

/// 同时实现 ChatFn 和 ChatJsonFn 的组合 Mock
pub struct MockCombo {
    pub chat_response: Mutex<String>,
    pub json_response: Mutex<Value>,
}

impl MockCombo {
    pub fn new(chat_response: impl Into<String>, json_response: Value) -> Self {
        Self {
            chat_response: Mutex::new(chat_response.into()),
            json_response: Mutex::new(json_response),
        }
    }
}

impl ChatFn for MockCombo {
    async fn call_chat(
        &self,
        _messages: Vec<LlmMessage>,
        _max_tokens: Option<u32>,
    ) -> Result<String> {
        Ok(self.chat_response.lock().unwrap().clone())
    }
}

impl ChatJsonFn for MockCombo {
    async fn call_json(
        &self,
        _messages: Vec<LlmMessage>,
        _max_tokens: Option<u32>,
    ) -> Result<Value> {
        Ok(self.json_response.lock().unwrap().clone())
    }
}

// ═══════════════════════════════════════════════════════
//  测试夹具：预定义的 LLM 响应数据
// ═══════════════════════════════════════════════════════

/// 标准的摘要响应 Markdown
pub fn mock_summary_response() -> String {
    r#"# 知识管理方法论 — 核心摘要

## 概述

本文系统介绍了知识管理的方法论框架，包括知识获取、组织、存储和应用四个核心环节。作者强调了结构化思维在知识管理中的基础性作用。

## 核心要点

1. 知识管理需要系统化的方法论支撑
2. 结构化思维是高效知识管理的基础
3. 卡片笔记法能够有效降低认知负荷
4. 知识图谱有助于发现隐性关联

## 结构

第一章：知识管理概述
第二章：结构化思维方法
第三章：卡片笔记技术
第四章：知识图谱构建
"#
    .to_string()
}

/// 标准的实体识别 JSON 响应
pub fn mock_entities_json() -> Value {
    serde_json::json!({
        "entities": [
            { "name": "知识管理", "type": "概念", "context": "系统化管理知识的学科" },
            { "name": "卡片笔记法", "type": "方法", "context": "将知识点拆分为独立卡片的方法" },
            { "name": "结构化思维", "type": "概念", "context": "将信息和知识进行结构化组织的方法" },
            { "name": "知识图谱", "type": "技术", "context": "用图结构表达知识之间关系的技术" }
        ]
    })
}

/// 标准的卡片生成响应（legacy 格式，可被 parse_single_type_cards 解析）
///
/// 格式：`---` 分隔卡片块，每块含 `标题：`、`ref：` 字段和 `#卡片类型` 标签。
/// 此格式匹配 generate_cards_legacy → parse_single_type_cards 的解析路径。
pub fn mock_cards_legacy_response() -> String {
    r#"---
标题：知识管理的四个核心环节
知识管理包括获取、组织、存储和应用四个环节，缺一不可。获取是入口，组织是基础，存储是保障，应用是目的。
ref：第一章 知识管理概述
#新知卡
---
标题：结构化思维
结构化思维是将信息和知识按照逻辑层次进行组织的方法，它帮助我们从混乱中建立秩序，从表面的现象中找到深层规律。
ref：第二章 结构化思维方法
#术语卡
---
标题：卡片笔记法降低认知负荷
卡片笔记法通过将大块知识拆分成独立的小卡片，每次只需关注一个知识点，从而显著降低认知负荷，提高学习效率。
ref：第三章 卡片笔记技术
#新知卡
"#
    .to_string()
}

/// 标准的卡片生成 JSON 响应（extract-then-assign 格式）
/// 注意：当前 mock 优先使用 legacy 格式，因为两种策略共用同一个 MockChat，
/// 而 extract-then-assign 无法解析后将自动回退到 legacy。
pub fn mock_cards_json() -> Value {
    serde_json::json!({
        "cards": [
            {
                "title": "知识管理的四个核心环节",
                "content": "知识管理包括获取、组织、存储和应用四个环节，缺一不可。获取是入口，组织是基础，存储是保障，应用是目的。",
                "card_type": "新知卡",
                "reference": "第一章 知识管理概述"
            },
            {
                "title": "结构化思维",
                "content": "结构化思维是将信息和知识按照逻辑层次进行组织的方法，它帮助我们从混乱中建立秩序，从表面的现象中找到深层规律。",
                "card_type": "术语卡",
                "reference": "第二章 结构化思维方法"
            },
            {
                "title": "卡片笔记法降低认知负荷",
                "content": "卡片笔记法通过将大块知识拆分成独立的小卡片，每次只需关注一个知识点，从而显著降低认知负荷，提高学习效率。",
                "card_type": "新知卡",
                "reference": "第三章 卡片笔记技术"
            }
        ]
    })
}

/// 标准的关系图谱 JSON 响应
pub fn mock_graph_json() -> Value {
    serde_json::json!({
        "relations": [
            {
                "source": "知识管理",
                "target": "结构化思维",
                "relation_type": "依赖",
                "evidence": "知识管理需要结构化的方法论支撑"
            },
            {
                "source": "卡片笔记法",
                "target": "结构化思维",
                "relation_type": "基于",
                "evidence": "卡片笔记法建立在结构化思维的基础上"
            },
            {
                "source": "知识图谱",
                "target": "知识管理",
                "relation_type": "应用于",
                "evidence": "知识图谱是知识管理的重要工具"
            }
        ]
    })
}

/// 最小但有效的摘要响应
pub fn mock_minimal_summary() -> String {
    "# 测试文档 — 核心摘要\n\n## 概述\n\n这是一个测试文档。\n\n## 核心要点\n\n1. 测试要点\n"
        .to_string()
}

/// 空实体的 JSON 响应
pub fn mock_empty_entities_json() -> Value {
    serde_json::json!({ "entities": [] })
}

/// 空卡片的 JSON 响应
pub fn mock_empty_cards_json() -> Value {
    serde_json::json!({ "cards": [] })
}

/// 空关系的 JSON 响应
pub fn mock_empty_graph_json() -> Value {
    serde_json::json!({ "relations": [] })
}

// ═══════════════════════════════════════════════════════
//  测试数据工厂
// ═══════════════════════════════════════════════════════

/// 构建标准测试卡片
pub fn make_test_card(title: &str, content: &str, card_type: CardType) -> Card {
    Card {
        title: title.to_string(),
        content: content.to_string(),
        card_type,
        reference: format!("来源-{}", title),
        unique_id: chrono::Local::now()
            .format("%Y%m%d%H%M%S")
            .to_string(),
        ..Default::default()
    }
}

/// 构建标准测试实体
pub fn make_test_entity(name: &str, entity_type: &str) -> Entity {
    Entity {
        name: name.to_string(),
        entity_type: entity_type.to_string(),
        context: String::new(),
    }
}

/// 构建标准测试关系
pub fn make_test_relation(source: &str, target: &str, relation_type: &str) -> Relation {
    Relation {
        source: source.to_string(),
        target: target.to_string(),
        relation_type: relation_type.to_string(),
        evidence: String::new(),
    }
}

/// 构建完整的 CompilationResult 用于输出测试
pub fn make_test_compilation_result() -> cardnote_compiler::models::CompilationResult {
    let summary = Summary {
        title: "测试书籍".to_string(),
        overview: "这是一本关于测试的书籍的概述。".to_string(),
        key_points: vec![
            "要点一：测试驱动开发".to_string(),
            "要点二：持续集成".to_string(),
        ],
        structure: "第一章 测试基础\n第二章 高级测试".to_string(),
    };

    let cards = vec![
        make_test_card("测试驱动开发", "测试驱动开发（TDD）是一种软件开发方法，要求在编写功能代码之前先编写测试代码。", CardType::Knowledge),
        make_test_card("单元测试", "单元测试是对软件中最小可测试单元进行检查和验证的测试方法。", CardType::Term),
        make_test_card(
            "代码质量名言",
            "测试代码和生产代码一样重要。",
            CardType::Quote,
        ),
        make_test_card("GitHub Actions", "GitHub Actions 是 GitHub 的 CI/CD 平台。", CardType::NewWord),
    ];

    let entities = vec![
        make_test_entity("TDD", "概念"),
        make_test_entity("GitHub Actions", "工具"),
    ];

    let relations = vec![
        make_test_relation("TDD", "单元测试", "包含"),
        make_test_relation("GitHub Actions", "TDD", "支持"),
    ];

    let graph = cardnote_compiler::models::KnowledgeGraph {
        entities,
        relations,
    };

    cardnote_compiler::models::CompilationResult {
        source_file: "/test/book.pdf".to_string(),
        summary,
        cards,
        graph,
        chunks: vec![],
        diagnostics: Default::default(),
    }
}

/// 简短测试文档
pub const TEST_DOCUMENT: &str = r#"# 知识管理入门

知识管理是一个系统化的过程，旨在帮助个人和组织有效地获取、组织、存储和应用知识。

## 什么是结构化思维

结构化思维是将复杂信息按照逻辑层次进行组织的方法。它帮助我们：
- 从混乱中找到秩序
- 识别信息之间的关联
- 建立清晰的思维模型

## 卡片笔记法

卡片笔记法是一种将知识点拆分为独立卡片的方法。每张卡片只包含一个知识点，便于后续检索和关联。

### 核心原则

1. 一张卡片只记录一个知识点
2. 卡片之间通过链接建立关联
3. 定期回顾和整理卡片

## 知识图谱

知识图谱通过图结构表达知识之间的关系，使得隐性关联更容易被发现。
"#;

/// 带中文标点和特殊字符的测试文档
pub const TEST_DOCUMENT_CHINESE: &str = r#"# 阅读的心智

阅读不仅是获取信息的过程，更是一种心智活动。

## 阅读的层次

莫提默·艾德勒（Mortimer Adler）在《如何阅读一本书》中提出了阅读的四个层次：
1. 基础阅读（Elementary Reading）
2. 检视阅读（Inspectional Reading）
3. 分析阅读（Analytical Reading）
4. 主题阅读（Syntopical Reading）

## 认知心理学视角

从认知心理学的角度看，阅读涉及：
- 注意力的分配与管理
- 工作记忆的负荷
- 长期记忆的编码与提取
- 元认知监控

## 深度阅读的价值

深度阅读能够：
- 培养批判性思维能力
- 增强同理心和理解力
- 提高专注力和耐心
- 促进创造性思维的发展
"#;

/// 模拟 prompt 加载函数（不依赖文件系统）
pub fn mock_load_prompt(name: &str) -> cardnote_compiler::error::Result<String> {
    match name {
        "summary" => Ok("请为以下文档生成摘要：\n\n{document}\n\n请包括标题、概述、核心要点和结构。".to_string()),
        "entity_extraction" => Ok("请从以下文档中提取实体：\n\n{document}\n\n请以 JSON 格式输出。".to_string()),
        "relation_graph" => Ok("请从以下文档中构建关系图谱：\n\n{document}\n\n已知实体：\n{entities}\n\n请以 JSON 格式输出。".to_string()),
        "knowledge_card" => Ok("请从以下文档中提取新知卡：\n\n{document}".to_string()),
        "term_card" => Ok("请从以下文档中提取术语卡：\n\n{document}".to_string()),
        "person_card" => Ok("请从以下文档中提取人物卡：\n\n{document}".to_string()),
        "quote_card" => Ok("请从以下文档中提取金句卡：\n\n{document}".to_string()),
        "action_card" => Ok("请从以下文档中提取行动卡：\n\n{document}".to_string()),
        "event_card" => Ok("请从以下文档中提取事件卡：\n\n{document}".to_string()),
        "counter_intuit_card" => Ok("请从以下文档中提取反常识卡：\n\n{document}".to_string()),
        "review_card" => Ok("请从以下文档中提取综述卡：\n\n{document}".to_string()),
        "graph_card" => Ok("请从以下文档中提取图示卡：\n\n{document}".to_string()),
        "new_word_card" => Ok("请从以下文档中提取新词卡：\n\n{document}".to_string()),
        "note_card" => Ok("请从以下文档中提取基础卡：\n\n{document}".to_string()),
        "index_card" => Ok("请从以下文档中提取索引卡：\n\n{document}".to_string()),
        _ => Err(cardnote_compiler::error::AppError::PromptLoad(
            format!("未知 prompt: {}", name),
        )),
    }
}
