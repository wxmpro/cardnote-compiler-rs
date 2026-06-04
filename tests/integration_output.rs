//! 输出生成集成测试 — 验证编译结果写入文件系统的完整流程
//!
//! 测试覆盖：
//! 1. 所有输出文件类型（summary、cards、graph、entities、quality report）
//! 2. 目录冲突解决
//! 3. 文件名安全处理
//! 4. 各类卡片格式

mod common;

use cardnote_compiler::models::{CardStatus, CardType, CompilationResult, Summary};
use cardnote_compiler::output::{
    sanitize_filename, save_cards_by_type, save_single, save_single_to_dir,
};
use common::*;

// ═══════════════════════════════════════════════════════
//  文件生成完整性测试
// ═══════════════════════════════════════════════════════

#[tokio::test]
async fn test_save_creates_all_required_files() {
    let result = make_test_compilation_result();
    let temp_dir = tempfile::tempdir().unwrap();
    let dir_path = temp_dir.path().to_string_lossy().to_string();

    let output_path = save_single(&result, &dir_path)
        .await
        .expect("save_single 应成功");

    let output_dir = std::path::Path::new(&output_path);

    // 验证所有必需文件
    let required_files = [
        "summary.md",
        "all_cards.md",
        "graph.mmd",
        "entities.md",
        "card_quality_report.md",
    ];
    for file in &required_files {
        assert!(
            output_dir.join(file).exists(),
            "文件 {} 应存在在 {}",
            file,
            output_path
        );
    }

    // 验证 cards/ 子目录
    assert!(output_dir.join("cards").exists());

    // 清理
    let _ = std::fs::remove_dir_all(&output_path);
}

#[tokio::test]
async fn test_save_all_card_types() {
    // 生成所有 12 种卡片的测试数据
    let all_card_types = vec![
        (CardType::Knowledge, "新知卡"),
        (CardType::Term, "术语卡"),
        (CardType::Person, "人物卡"),
        (CardType::Quote, "金句卡"),
        (CardType::Event, "事件卡"),
        (CardType::Action, "行动卡"),
        (CardType::Graph, "图示卡"),
        (CardType::NewWord, "新词卡"),
        (CardType::Note, "基础卡"),
        (CardType::Index, "索引卡"),
        (CardType::CounterIntuit, "反常识卡"),
        (CardType::Review, "综述卡"),
    ];

    let cards: Vec<_> = all_card_types
        .iter()
        .map(|(ct, name)| {
            let mut card = make_test_card(
                &format!("{}测试", name),
                &format!("这是一张{}的测试内容，包含了足够多的描述文字来验证卡片输出格式的正确性。", name),
                ct.clone(),
            );
            // 为金句卡设置扩展字段
            if *ct == CardType::Quote {
                card.original_text = "这是一句原文引用。".to_string();
                card.source = "《测试之书》".to_string();
                card.paraphrase = "这句话的含义可以理解为...".to_string();
            }
            card
        })
        .collect();

    let temp_dir = tempfile::tempdir().unwrap();
    save_cards_by_type(temp_dir.path(), &cards)
        .await
        .expect("按类型保存卡片应成功");

    // 验证每种卡片类型都有对应的文件
    for (_, name) in &all_card_types {
        let filename = format!("{}.md", name);
        let filepath = temp_dir.path().join(&filename);
        assert!(
            filepath.exists(),
            "卡片类型文件 {} 应存在",
            filename
        );

        let content = std::fs::read_to_string(&filepath).unwrap();
        assert!(
            content.contains("**标题：**"),
            "{} 应包含卡片标题",
            filename
        );
        assert!(
            content.contains("**uuid：**"),
            "{} 应包含 UUID",
            filename
        );
    }
}

#[tokio::test]
async fn test_save_quote_card_special_format() {
    let quote_card = cardnote_compiler::models::Card {
        title: "关于勇气的名言".to_string(),
        content: "".to_string(),
        card_type: CardType::Quote,
        original_text: "「勇气不是没有恐惧，而是战胜恐惧。」".to_string(),
        source: "纳尔逊·曼德拉".to_string(),
        paraphrase: "真正的勇气在于面对恐惧时仍然前行。".to_string(),
        related_cards: vec!["恐惧心理学".to_string(), "成长型思维".to_string()],
        reference: "《曼德拉自传》".to_string(),
        unique_id: "20240101120000".to_string(),
        ..Default::default()
    };

    let md = quote_card.to_markdown();

    // 金句卡应包含特殊字段
    assert!(md.contains("**原文：**"));
    assert!(md.contains("「勇气不是没有恐惧，而是战胜恐惧。」"));
    assert!(md.contains("**出处：**"));
    assert!(md.contains("纳尔逊·曼德拉"));
    assert!(md.contains("**仿写：**"));
    assert!(md.contains("真正的勇气在于面对恐惧时仍然前行。"));
    assert!(md.contains("关联卡片"));
    assert!(md.contains("恐惧心理学"));
    assert!(md.contains("# 金句卡"));
}

#[tokio::test]
async fn test_save_summary_with_full_structure() {
    let summary = Summary {
        title: "深入理解计算机系统".to_string(),
        overview: "本书从程序员视角系统讲解了计算机系统的核心概念，涵盖数据表示、机器级编程、处理器架构、内存层次、链接和异常控制流等主题。".to_string(),
        key_points: vec![
            "信息的位表示与操作是计算机系统的基础".to_string(),
            "理解处理器架构有助于编写高效的代码".to_string(),
            "内存层次结构对程序性能有决定性影响".to_string(),
        ],
        structure: "第一部分：程序结构和执行\n第二部分：在系统上运行程序\n第三部分：程序间的交互和通信".to_string(),
    };

    let md = summary.to_markdown();

    assert!(md.contains("# 深入理解计算机系统 — 核心摘要"));
    assert!(md.contains("## 概述"));
    assert!(md.contains("程序员视角"));
    assert!(md.contains("## 核心要点"));
    assert!(md.contains("1. 信息的位表示"));
    assert!(md.contains("2. 理解处理器架构"));
    assert!(md.contains("3. 内存层次结构"));
    assert!(md.contains("## 结构"));
    assert!(md.contains("第一部分：程序结构和执行"));
}

// ═══════════════════════════════════════════════════════
//  目录冲突解决测试
// ═══════════════════════════════════════════════════════

#[tokio::test]
async fn test_output_dir_conflict_resolution() {
    let temp_dir = tempfile::tempdir().unwrap();
    let base = temp_dir.path().to_string_lossy().to_string();

    // 第一次保存
    let result1 = make_test_compilation_result();
    let path1 = save_single(&result1, &base).await.unwrap();

    // 第二次保存，时间戳可能相同
    let result2 = make_test_compilation_result();
    let path2 = save_single(&result2, &base).await.unwrap();

    // 路径不应相同
    assert_ne!(path1, path2);
    assert!(std::path::Path::new(&path2).exists());

    // 清理
    let _ = std::fs::remove_dir_all(&path1);
    let _ = std::fs::remove_dir_all(&path2);
}

// ═══════════════════════════════════════════════════════
//  文件名安全处理测试
// ═══════════════════════════════════════════════════════

#[test]
fn test_sanitize_filename_special_chars() {
    // Windows 和 Unix 非法字符
    assert_eq!(sanitize_filename("a/b:c"), "a_b_c");
    assert_eq!(sanitize_filename("test|file?"), "test_file_");
    assert_eq!(sanitize_filename("hello*world"), "hello_world");
    assert_eq!(sanitize_filename("file<name>"), "file_name_");
}

#[test]
fn test_sanitize_filename_chinese_punctuation() {
    assert_eq!(sanitize_filename("书名：副标题"), "书名_副标题");
    assert_eq!(sanitize_filename("问题？答案"), "问题_答案");
    assert_eq!(sanitize_filename("A＜B＞C"), "A_B_C");
}

#[test]
fn test_sanitize_filename_trim_whitespace() {
    assert_eq!(sanitize_filename("  标题  "), "标题");
    assert_eq!(sanitize_filename("\t内容\n"), "内容");
}

#[test]
fn test_sanitize_filename_empty_and_special() {
    assert_eq!(sanitize_filename(""), "");
    // 全为非法字符
    assert_eq!(sanitize_filename("?*:/"), "____");
}

// ═══════════════════════════════════════════════════════
//  卡片质量报告测试
// ═══════════════════════════════════════════════════════

#[tokio::test]
async fn test_quality_report_with_mixed_status() {
    use cardnote_compiler::models::{Card, CardType};

    let cards = vec![
        Card {
            title: "通过的卡片".to_string(),
            content: "内容".to_string(),
            card_type: CardType::Knowledge,
            status: CardStatus::Accepted,
            quality_score: 0.95,
            ..Default::default()
        },
        Card {
            title: "需重试的卡片".to_string(),
            content: "内容".to_string(),
            card_type: CardType::Term,
            status: CardStatus::NeedsRetry,
            quality_score: 0.3,
            reject_reason: "内容过短".to_string(),
            ..Default::default()
        },
        Card {
            title: "已降级的卡片".to_string(),
            content: "内容".to_string(),
            card_type: CardType::Knowledge,
            status: CardStatus::Degraded,
            quality_score: 0.5,
            ..Default::default()
        },
    ];

    let temp_dir = tempfile::tempdir().unwrap();
    let result = CompilationResult {
        source_file: "test.pdf".to_string(),
        summary: Summary::default(),
        cards: cards.clone(),
        graph: cardnote_compiler::models::KnowledgeGraph {
            entities: vec![],
            relations: vec![],
        },
        chunks: vec![],
        diagnostics: Default::default(),
    };

    save_single_to_dir(&result, temp_dir.path())
        .await
        .unwrap();

    let report_path = temp_dir.path().join("card_quality_report.md");
    let report = std::fs::read_to_string(&report_path).unwrap();

    assert!(report.contains("需关注卡片"));
    assert!(report.contains("需重试的卡片"));
    assert!(report.contains("已降级的卡片"));
    assert!(report.contains("需重试"));
}

// ═══════════════════════════════════════════════════════
//  编译诊断报告测试
// ═══════════════════════════════════════════════════════

#[tokio::test]
async fn test_diagnostics_report_with_failures() {
    use cardnote_compiler::models::{
        CompilationDiagnostics, CompilationResult, StageDegradation, StageFail, StageRetry,
    };

    let diagnostics = CompilationDiagnostics {
        failures: vec![
            StageFail {
                stage: "entities".to_string(),
                error: "JSON 解析失败".to_string(),
                retry_count: 3,
                final_status: "failed".to_string(),
            },
            StageFail {
                stage: "graph".to_string(),
                error: "API 超时".to_string(),
                retry_count: 3,
                final_status: "failed".to_string(),
            },
        ],
        degradations: vec![StageDegradation {
            stage: "cards".to_string(),
            reason: "仅生成 2 张卡片，预期 5 张".to_string(),
            expected_count: 5,
            actual_count: 2,
        }],
        retries: vec![StageRetry {
            stage: "summary".to_string(),
            retry_count: 2,
            success: true,
        }],
    };

    let result = CompilationResult {
        source_file: "test.pdf".to_string(),
        summary: Summary::default(),
        cards: vec![],
        graph: cardnote_compiler::models::KnowledgeGraph {
            entities: vec![],
            relations: vec![],
        },
        chunks: vec![],
        diagnostics,
    };

    let temp_dir = tempfile::tempdir().unwrap();
    save_single_to_dir(&result, temp_dir.path())
        .await
        .unwrap();

    let diag_path = temp_dir.path().join("compile_diagnostics.md");
    assert!(diag_path.exists(), "诊断文件应被创建");

    let diag_content = std::fs::read_to_string(&diag_path).unwrap();
    assert!(diag_content.contains("编译诊断报告"));
    assert!(diag_content.contains("失败阶段"));
    assert!(diag_content.contains("entities"));
    assert!(diag_content.contains("JSON 解析失败"));
    assert!(diag_content.contains("降级阶段"));
    assert!(diag_content.contains("重试结果"));
}

#[tokio::test]
async fn test_no_diagnostics_report_when_all_ok() {
    // 全部成功的编译不应生成诊断报告
    let result = make_test_compilation_result(); // diagnostics 为 Default::default() = 全部空

    let temp_dir = tempfile::tempdir().unwrap();
    save_single_to_dir(&result, temp_dir.path())
        .await
        .unwrap();

    let diag_path = temp_dir.path().join("compile_diagnostics.md");
    // 根据 output.rs 逻辑: 仅当 failures/degrations/retries 非空时才写入
    assert!(!diag_path.exists(), "全部成功的编译不应生成诊断报告");
}

// ═══════════════════════════════════════════════════════
//  Markdown 转义测试
// ═══════════════════════════════════════════════════════

#[test]
fn test_card_markdown_escape_special_chars() {
    // 内容中含有可能破坏 Markdown 格式的字符
    let card = cardnote_compiler::models::Card {
        title: "测试---标题".to_string(),
        content: "这是---内容，包含 # 号。".to_string(),
        card_type: CardType::Knowledge,
        reference: "ref---来源".to_string(),
        unique_id: "20240101120000".to_string(),
        ..Default::default()
    };

    let md = card.to_markdown();
    // --- 应被转义
    assert!(md.contains("\\---"));
}

#[test]
fn test_mermaid_graph_with_special_entities() {
    let graph = cardnote_compiler::models::KnowledgeGraph {
        entities: vec![],
        relations: vec![
            cardnote_compiler::models::Relation {
                source: "#标签A".to_string(),
                target: "\"节点B\"".to_string(),
                relation_type: "包含".to_string(),
                evidence: "".to_string(),
            },
        ],
    };

    let mermaid = graph.to_mermaid();
    // # 应转义
    assert!(mermaid.contains("\\#"));
    // " 应替换为 '
    assert!(mermaid.contains("'节点B'"));
    assert!(!mermaid.contains("\"节点B\""));
}

// ═══════════════════════════════════════════════════════
//  Card to_markdown 所有类型覆盖
// ═══════════════════════════════════════════════════════

#[test]
fn test_all_card_types_to_markdown() {
    let all_types = [
        CardType::Knowledge,
        CardType::Term,
        CardType::Person,
        CardType::Quote,
        CardType::Event,
        CardType::Action,
        CardType::Graph,
        CardType::NewWord,
        CardType::Note,
        CardType::Index,
        CardType::CounterIntuit,
        CardType::Review,
    ];

    for ct in &all_types {
        let card = cardnote_compiler::models::Card {
            title: format!("{} 测试", ct.as_str()),
            content: format!("这是 {} 的内容。", ct.as_str()),
            card_type: ct.clone(),
            reference: "来源".to_string(),
            unique_id: "20240101120000".to_string(),
            ..Default::default()
        };

        let md = card.to_markdown();
        // 每种卡片至少应包含标题和内容
        assert!(md.contains("**标题：**"), "{} 应包含标题标记", ct);
        assert!(md.contains("**uuid：**"), "{} 应包含 UUID", ct);

        // 验证卡片类型名称在 markdown 中
        assert!(
            md.contains(&format!("# {}", ct.as_str())),
            "{} 的 markdown 应包含卡片类型名称 # {}",
            ct.as_str(),
            ct.as_str()
        );
    }
}

// ═══════════════════════════════════════════════════════
//  分块信息输出测试
// ═══════════════════════════════════════════════════════

#[tokio::test]
async fn test_output_with_chunks_info() {
    use cardnote_compiler::models::ChunkInfo;

    let mut result = make_test_compilation_result();
    result.chunks = vec![
        ChunkInfo {
            title_path: "第一章 > 第一节".to_string(),
            size: 5000,
            entities: 10,
            cards: 5,
            relations: 3,
        },
        ChunkInfo {
            title_path: "第一章 > 第二节".to_string(),
            size: 8000,
            entities: 15,
            cards: 8,
            relations: 5,
        },
    ];

    let temp_dir = tempfile::tempdir().unwrap();
    save_single_to_dir(&result, temp_dir.path())
        .await
        .unwrap();

    let chunks_path = temp_dir.path().join("chunks.md");
    assert!(chunks_path.exists(), "有分块信息时应生成 chunks.md");

    let content = std::fs::read_to_string(&chunks_path).unwrap();
    assert!(content.contains("分块信息"));
    assert!(content.contains("第一章 > 第一节"));
    assert!(content.contains("第一章 > 第二节"));
    assert!(content.contains("5000 字符"));
    assert!(content.contains("8000 字符"));
}

#[tokio::test]
async fn test_output_without_chunks_no_chunks_file() {
    let result = make_test_compilation_result(); // chunks 为空

    let temp_dir = tempfile::tempdir().unwrap();
    save_single_to_dir(&result, temp_dir.path())
        .await
        .unwrap();

    let chunks_path = temp_dir.path().join("chunks.md");
    assert!(!chunks_path.exists(), "无分块信息时不应生成 chunks.md");
}
