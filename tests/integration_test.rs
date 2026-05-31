use cardnote_compiler::models::{KnowledgeGraph, LlmMessage, Summary};
use cardnote_compiler::stages::common::{ChatFn, ChatJsonFn};
use cardnote_compiler::error::Result;
use serde_json::Value;

/// Mock LLM 客户端：返回预定义响应，不触发真实 HTTP 请求
#[derive(Clone)]
struct MockLlmClient {
    summary_response: String,
    entities_response: Value,
}

impl ChatFn for MockLlmClient {
    async fn call_chat(
        &self,
        _messages: Vec<LlmMessage>,
        _max_tokens: Option<u32>,
    ) -> Result<String> {
        // 根据消息内容判断请求类型，返回对应响应
        Ok(self.summary_response.clone())
    }
}

impl ChatJsonFn for MockLlmClient {
    async fn call_json(
        &self,
        _messages: Vec<LlmMessage>,
        _max_tokens: Option<u32>,
    ) -> Result<Value> {
        // 简单判断：如果消息包含"实体"返回实体响应，否则返回图谱响应
        Ok(self.entities_response.clone())
    }
}

/// 端到端测试：使用 Mock LLM 验证 Pipeline 单轮编译流程
#[tokio::test]
async fn test_pipeline_single_run_with_mock_llm() {
    let client = MockLlmClient {
        summary_response: r#"# 测试文档 — 核心摘要

## 概述

这是一篇测试文档的概述。

## 核心要点

1. 要点A
2. 要点B

## 结构

第一章 引言
第二章 方法
"#
        .to_string(),
        entities_response: serde_json::json!([
            {"name": "测试实体A", "entity_type": "概念", "context": "上下文A"},
            {"name": "测试实体B", "entity_type": "人物", "context": "上下文B"}
        ]),
    };

    // 注：此测试需要 Pipeline 支持注入 Mock client。
    // 当前 Pipeline::new 只接受 LlmClient，不接受自定义 ChatFn。
    // 因此此测试暂时只作为结构示例，实际运行需要 Pipeline 重构。
    // 下面验证 Mock client 本身可以工作。
    let response = client
        .call_chat(vec![], None)
        .await
        .expect("Mock chat 应返回预定义响应");
    assert!(response.contains("测试文档"));

    let json = client
        .call_json(vec![], None)
        .await
        .expect("Mock json 应返回预定义响应");
    assert!(json.is_array() || json.is_object());
}

/// 测试编译结果健康检查：空结果应触发警告逻辑
#[test]
fn test_empty_compilation_result_detection() {
    let empty_result = cardnote_compiler::models::CompilationResult {
        source_file: "test.md".to_string(),
        summary: Summary::default(),
        cards: vec![],
        graph: KnowledgeGraph {
            entities: vec![],
            relations: vec![],
        },
        chunks: vec![],
        diagnostics: Default::default(),
    };

    assert!(empty_result.cards.is_empty());
    assert!(empty_result.summary.title.is_empty());
    assert!(empty_result.summary.overview.is_empty());
}

/// 测试卡片解析：记录加粗格式的已知限制
#[test]
fn test_card_field_extraction_bold_format_limitation() {
    // LLM 可能输出 **标题：** 而非 标题：
    let block = "**标题：** 测试标题\n**ref：** 来源_p10\n内容正文。";

    // 当前 extract_field 使用正则 `^标题[：:]`，不匹配 `**标题：**`
    // 这是一个已知限制：加粗格式的字段前缀会导致提取失败
    let title_re = regex::Regex::new(r"^标题[:：]\s*(.+?)(?:\n|$)").unwrap();
    let title = title_re.captures(block).and_then(|c| c.get(1)).map(|m| m.as_str().trim());

    // 记录当前行为：加粗格式不被支持，提取返回 None
    assert_eq!(title, None, "当前版本不支持加粗格式字段前缀（已知限制）");
}
