//! 端到端 Pipeline 测试 — 验证完整编译链路
//!
//! 这些测试模拟从文档输入到结构化输出的全流程：
//! 1. 文档 → LLM 响应解析 → 结构化数据
//! 2. 数据 → 文件系统输出
//! 3. 去重、质量过滤、格式转换
//!
//! 所有测试不依赖真实 HTTP，使用 Mock ChatFn/ChatJsonFn。

mod common;

use cardnote_compiler::dedup::semantic_dedup;
use cardnote_compiler::models::CardType;
use cardnote_compiler::output::save_single_to_dir;
use cardnote_compiler::pipeline::fnv1a_hash_str;
use cardnote_compiler::quality::CardLintConfig;
use cardnote_compiler::stages::entities::{extract_entities, unify_entities};
use common::*;

// ═══════════════════════════════════════════════════════
//  E2E: 去重 → 质量过滤 → 输出
// ═══════════════════════════════════════════════════════

#[tokio::test]
async fn test_e2e_dedup_filter_output() {
    // 1. 创建含重复卡片和低质量卡片的卡片集合
    let cards = vec![
        make_test_card(
            "知识管理入门",
            "知识管理是一个系统化过程，包括获取、组织、存储和应用四个环节。每个环节都至关重要，共同构成了完整的方法论体系。",
            CardType::Knowledge,
        ),
        make_test_card(
            "知识管理概述",
            "知识管理是一个系统化过程，包括获取、组织、存储和应用四个环节。每个环节都至关重要，共同构成了完整的方法论体系。",
            CardType::Knowledge,
        ),
        make_test_card(
            "结构化思维基础",
            "结构化思维是对信息进行分层和分类的认知方法，它帮助我们在复杂信息中找到秩序。",
            CardType::Knowledge,
        ),
    ];

    // 2. 去重
    let avg_len = cards.iter().map(|c| c.content.chars().count()).sum::<usize>() / cards.len();
    let config = cardnote_compiler::dedup::adaptive_dedup_config(avg_len);
    let dedup_result = semantic_dedup(&cards, &config);

    // 前两张高度相似应被合并
    assert_eq!(dedup_result.stats.merged_groups, 1);
    assert_eq!(dedup_result.stats.unique_count, 2);

    // 3. 质量过滤
    let lint_config = CardLintConfig::default();
    let (filtered, _stats) =
        cardnote_compiler::quality::filter_cards_with_source(
            &dedup_result.cards,
            TEST_DOCUMENT,
            &lint_config,
        );

    // 验证过滤不会 panic
    assert!(filtered.len() <= dedup_result.stats.unique_count);

    // 4. 输出到文件
    let mut result = make_test_compilation_result();
    result.cards = filtered;

    let temp_dir = tempfile::tempdir().unwrap();
    save_single_to_dir(&result, temp_dir.path())
        .await
        .expect("保存应成功");

    // 5. 验证输出文件完整性（Unified 模式输出 all_cards.md + entities.md + card_quality_report.md）
    assert!(temp_dir.path().join("all_cards.md").exists());
    assert!(temp_dir.path().join("entities.md").exists());
    assert!(temp_dir.path().join("card_quality_report.md").exists());
}

// ═══════════════════════════════════════════════════════
//  E2E: 知识图谱 → Mermaid → 文件
// ═══════════════════════════════════════════════════════

#[test]
fn test_e2e_graph_to_mermaid_to_file() {
    use cardnote_compiler::models::{Entity, KnowledgeGraph, Relation};

    // 1. 构建知识图谱数据
    let graph = KnowledgeGraph {
        entities: vec![
            Entity {
                name: "阅读".to_string(),
                entity_type: "活动".to_string(),
                context: "人类获取信息的主要方式".to_string(),
            },
            Entity {
                name: "认知".to_string(),
                entity_type: "心理过程".to_string(),
                context: "".to_string(),
            },
            Entity {
                name: "元认知".to_string(),
                entity_type: "心理过程".to_string(),
                context: "对认知的认知".to_string(),
            },
        ],
        relations: vec![
            Relation {
                source: "阅读".to_string(),
                target: "认知".to_string(),
                relation_type: "激活".to_string(),
                evidence: "阅读过程需要认知能力的参与".to_string(),
            },
            Relation {
                source: "阅读".to_string(),
                target: "元认知".to_string(),
                relation_type: "促进".to_string(),
                evidence: "深度阅读能促进元认知能力发展".to_string(),
            },
            Relation {
                source: "元认知".to_string(),
                target: "认知".to_string(),
                relation_type: "监控".to_string(),
                evidence: "".to_string(),
            },
        ],
    };

    // 2. 生成 Mermaid
    let mermaid = graph.to_mermaid();
    assert!(mermaid.starts_with("graph TD"));
    assert!(mermaid.contains("阅读"));
    assert!(mermaid.contains("认知"));
    assert!(mermaid.contains("元认知"));
    assert!(mermaid.contains("激活"));
    assert!(mermaid.contains("促进"));
    assert!(mermaid.contains("监控"));

    // 3. 写入文件并验证
    let temp_dir = tempfile::tempdir().unwrap();
    let graph_path = temp_dir.path().join("graph.mmd");
    std::fs::write(&graph_path, &mermaid).unwrap();

    let read_back = std::fs::read_to_string(&graph_path).unwrap();
    assert_eq!(read_back, mermaid);
}

// ═══════════════════════════════════════════════════════
//  E2E: 所有卡片类型 → Markdown → 文件
// ═══════════════════════════════════════════════════════

#[tokio::test]
async fn test_e2e_all_cards_to_markdown_files() {
    use cardnote_compiler::models::Card;

    let all_cards = vec![
        Card {
            title: "苏格拉底".to_string(),
            content: "古希腊哲学家，西方哲学的奠基人之一。".to_string(),
            card_type: CardType::Person,
            reference: "《西方哲学史》".to_string(),
            unique_id: "20240101120000".to_string(),
            ..Default::default()
        },
        Card {
            title: "认识论".to_string(),
            content: "研究知识的本质、起源和范围的哲学分支。".to_string(),
            card_type: CardType::Term,
            reference: "《哲学导论》".to_string(),
            unique_id: "20240101120001".to_string(),
            ..Default::default()
        },
        Card {
            title: "我思故我在".to_string(),
            content: "".to_string(),
            card_type: CardType::Quote,
            original_text: "「我思故我在」（Cogito, ergo sum）".to_string(),
            source: "笛卡尔".to_string(),
            paraphrase: "思考的行为本身就证明了自己的存在。".to_string(),
            reference: "《第一哲学沉思集》".to_string(),
            unique_id: "20240101120002".to_string(),
            ..Default::default()
        },
        Card {
            title: "每日反思".to_string(),
            content: "每天花15分钟反思当天的学习和决策。".to_string(),
            card_type: CardType::Action,
            reference: "".to_string(),
            unique_id: "20240101120003".to_string(),
            ..Default::default()
        },
    ];

    let temp_dir = tempfile::tempdir().unwrap();
    cardnote_compiler::output::save_cards_by_type(temp_dir.path(), &all_cards)
        .await
        .expect("保存不应失败");

    // 验证按类型分组的文件
    assert!(temp_dir.path().join("人物卡.md").exists());
    assert!(temp_dir.path().join("术语卡.md").exists());
    assert!(temp_dir.path().join("金句卡.md").exists());
    assert!(temp_dir.path().join("行动卡.md").exists());

    // 验证金句卡格式
    let quote_content = std::fs::read_to_string(temp_dir.path().join("金句卡.md")).unwrap();
    assert!(quote_content.contains("**原文：**"));
    assert!(quote_content.contains("笛卡尔"));
    assert!(quote_content.contains("**仿写：**"));

    // 验证人物卡格式
    let person_content = std::fs::read_to_string(temp_dir.path().join("人物卡.md")).unwrap();
    assert!(person_content.contains("苏格拉底"));
    assert!(person_content.contains("古希腊"));
}

// ═══════════════════════════════════════════════════════
//  E2E: 错误恢复 — 各阶段失败不影响后续
// ═══════════════════════════════════════════════════════

#[tokio::test]
async fn test_e2e_error_recovery_entities_failure() {
    // 模拟实体阶段返回无效 JSON
    let invalid_json = MockChatJson::new(serde_json::json!({"wrong": "format"}));
    let dummy_chat = MockChat::new("");

    let result = extract_entities(TEST_DOCUMENT, &dummy_chat, &invalid_json, &mock_load_prompt)
        .await;

    // 不应 panic，应优雅降级
    match result {
        Ok(entities) => {
            // OK: 返回空列表
            assert!(entities.is_empty(), "无效格式应返回空列表");
        }
        Err(_) => {
            // 也可能是 Err，但不应该 panic
        }
    }
}

// ═══════════════════════════════════════════════════════
//  E2E: FNV-1a 哈希稳定性
// ═══════════════════════════════════════════════════════

#[test]
fn test_e2e_fnv1a_hash_used_throughout_pipeline() {
    // FNV-1a 哈希用于：缓存 key、内容去重
    let hash1 = fnv1a_hash_str(TEST_DOCUMENT);
    let hash2 = fnv1a_hash_str(TEST_DOCUMENT);
    assert_eq!(hash1, hash2);

    // 不同内容不同哈希
    let hash3 = fnv1a_hash_str("different content");
    assert_ne!(hash1, hash3);

    // 哈希长度固定
    assert_eq!(hash1.len(), 16);
    assert!(hash1.chars().all(|c| c.is_ascii_hexdigit()));
}

// ═══════════════════════════════════════════════════════
//  E2E: CompilationResult → save_single → 验证所有文件
// ═══════════════════════════════════════════════════════

#[tokio::test]
async fn test_e2e_full_output_integrity() {
    let result = make_test_compilation_result();
    let temp_dir = tempfile::tempdir().unwrap();

    save_single_to_dir(&result, temp_dir.path())
        .await
        .expect("保存应成功");

    // 检查每个文件的内容完整性（Unified 模式产物：all_cards.md + entities.md + card_quality_report.md）
    let checks: Vec<(&str, &str)> = vec![
        ("all_cards.md", "测试驱动开发"),
        ("all_cards.md", "单元测试"),
        ("all_cards.md", "代码质量名言"),
        ("entities.md", "实体列表"),
        ("card_quality_report.md", "卡片质量报告"),
    ];

    for (filename, expected_content) in &checks {
        let filepath = temp_dir.path().join(filename);
        assert!(
            filepath.exists(),
            "文件 {} 应存在",
            filename
        );
        let content = std::fs::read_to_string(&filepath).unwrap();
        assert!(
            content.contains(expected_content),
            "文件 {} 应包含 '{}'，实际内容: {}",
            filename,
            expected_content,
            &content[..std::cmp::min(200, content.len())]
        );
    }
}
