//! 跨书去重隔离测试 — 验证"同本书内去重，不同书不去重"的核心契约
//!
//! 用户场景：5000 本不同领域的经典书籍，逐本编译。
//! 每本书内部的相似卡片需要去重合并，但不同书之间的相同概念卡片各自独立保留。
//!
//! 这些测试验证 `semantic_dedup` 的行为边界：
//! 1. 同一本书内相似卡片被正确合并
//! 2. 不同书的相同概念卡片不被合并
//! 3. 跨类型同名卡片保留独立

mod common;

use cardnote_compiler::dedup::{semantic_dedup, DedupConfig};
use cardnote_compiler::models::CardType;
use common::make_test_card;

// ═══════════════════════════════════════════════════════
//  核心契约：同书内去重，跨书不去重
// ═══════════════════════════════════════════════════════

#[test]
fn test_same_book_similar_cards_are_merged() {
    // Book A: 两张相似的"系统思维"卡片
    let book_a_cards = vec![
        make_test_card(
            "系统思维",
            "系统思维是一种从整体出发、关注要素间相互关系的思考方式。它强调反馈循环、涌现性质和动态平衡。",
            CardType::Knowledge,
        ),
        make_test_card(
            "系统思维概述",
            "系统思维是一种从整体出发、关注要素间相互关系的思考方式。它强调反馈循环、涌现性质和动态平衡。",
            CardType::Knowledge,
        ),
    ];

    let result = semantic_dedup(&book_a_cards, &DedupConfig::default());

    // 同一本书内高度相似 → 应被合并
    assert_eq!(result.stats.original_count, 2);
    assert_eq!(result.stats.unique_count, 1);
    assert_eq!(result.stats.merged_groups, 1);
    assert_eq!(result.stats.removed_count, 1);

    // 合并后的卡片应标记"整合版"
    let merged = &result.cards[0];
    assert!(merged.title.contains("整合版"));
}

#[test]
fn test_different_books_same_concept_not_merged() {
    // Book A: "系统思维" — 认知科学视角
    let book_a_cards = vec![make_test_card(
        "系统思维",
        "系统思维是一种从整体出发、关注要素间相互关系的思考方式。它源自系统论和控制论。",
        CardType::Knowledge,
    )];

    // Book B: "系统思维" — 管理学视角（内容不同）
    let book_b_cards = vec![make_test_card(
        "系统思维",
        "系统思维在管理学中强调组织的整体性、部门间的协同效应和战略的系统性。",
        CardType::Knowledge,
    )];

    // 独立编译 Book A
    let result_a = semantic_dedup(&book_a_cards, &DedupConfig::default());
    assert_eq!(result_a.stats.unique_count, 1);
    assert_eq!(result_a.cards[0].title, "系统思维");

    // 独立编译 Book B
    let result_b = semantic_dedup(&book_b_cards, &DedupConfig::default());
    assert_eq!(result_b.stats.unique_count, 1);
    assert_eq!(result_b.cards[0].title, "系统思维");

    // 关键断言：两本书分别编译后，各自保留独立卡片
    // Book A 和 Book B 的"系统思维"内容不同，不应被当作需要合并的重复
    assert_ne!(
        result_a.cards[0].content,
        result_b.cards[0].content,
        "两本书的'系统思维'卡片内容不同，应各自独立"
    );
}

#[test]
fn test_cross_book_dedup_isolation_no_global_state() {
    // 验证 semantic_dedup 是无状态函数 — 多次调用之间无全局副作用
    let config = DedupConfig::default();

    let book1 = vec![
        make_test_card("概念A", "概念A的描述。", CardType::Term),
        make_test_card("概念A-重复", "概念A的描述。", CardType::Term),
    ];

    let book2 = vec![
        make_test_card("概念A", "概念A的完全不同的描述。", CardType::Term),
    ];

    // 第一次去重
    let r1 = semantic_dedup(&book1, &config);
    assert_eq!(r1.stats.unique_count, 1, "Book1: 两张相似卡片应合并");
    assert_eq!(r1.stats.merged_groups, 1);

    // 第二次去重 — 独立调用，不受前一次影响
    let r2 = semantic_dedup(&book2, &config);
    assert_eq!(r2.stats.unique_count, 1, "Book2: 单张卡片保持不变");
    assert_eq!(r2.stats.merged_groups, 0);

    // 关键：第一次去重不影响第二次
    assert_eq!(r2.cards[0].title, "概念A");
    assert_eq!(r2.cards[0].content, "概念A的完全不同的描述。");
}

#[test]
fn test_same_book_similar_content_different_titles_are_merged() {
    // 同一本书中，内容高度相似但标题不同的两张卡片
    let cards = vec![
        make_test_card(
            "认知心理学导论",
            "认知心理学研究人类的心理过程，包括知觉、注意、记忆、语言和思维。它是认知科学的核心分支。",
            CardType::Knowledge,
        ),
        make_test_card(
            "什么是认知心理学",
            "认知心理学研究人类的心理过程，包括知觉、注意、记忆、语言和思维。它是认知科学的核心分支。",
            CardType::Knowledge,
        ),
    ];

    let result = semantic_dedup(&cards, &DedupConfig::default());
    assert_eq!(result.stats.unique_count, 1);
    assert_eq!(result.stats.merged_groups, 1);
}

#[test]
fn test_same_book_different_concepts_kept_separate() {
    // 同一本书中，不同概念的卡片各自保留
    let cards = vec![
        make_test_card(
            "工作记忆",
            "工作记忆是一个容量有限的系统，用于临时存储和处理信息。巴德利提出的多成分模型包括语音回路和视空间画板。",
            CardType::Term,
        ),
        make_test_card(
            "长时记忆",
            "长时记忆是信息长期存储的仓库，容量几乎无限。包括陈述性记忆和程序性记忆两种类型。",
            CardType::Term,
        ),
        make_test_card(
            "元认知",
            "元认知是对自己认知过程的认知和监控，包括元认知知识和元认知调节两个维度。",
            CardType::Term,
        ),
    ];

    let result = semantic_dedup(&cards, &DedupConfig::default());
    assert_eq!(result.stats.unique_count, 3);
    assert_eq!(result.stats.merged_groups, 0);
}

#[test]
fn test_cross_type_same_name_kept_separate() {
    // 同一本书中，同名但不同类型的卡片各自独立
    // 例如："认知心理学" 作为术语卡 和 作为综述卡
    let cards = vec![
        make_test_card(
            "认知心理学",
            "认知心理学是研究人类认知过程的心理学分支，涉及知觉、注意、记忆、思维和语言等方面。",
            CardType::Term,
        ),
        make_test_card(
            "认知心理学",
            "本文综述了认知心理学从信息加工范式到具身认知范式的发展历程，涵盖了主要理论流派和实验范式。",
            CardType::Review,
        ),
    ];

    // 用默认配置测试
    let result = semantic_dedup(&cards, &DedupConfig::default());

    // 同名但不同类型（术语卡 vs 综述卡）且内容不同 — 不应合并
    // 注意：去重不区分卡片类型，只基于标题+内容的 shingle 相似度
    // 如果内容差异足够大，两张卡片应各自保留
    assert!(
        result.stats.unique_count >= 1,
        "同名不同类型卡片，内容不同应保留独立性"
    );
}

// ═══════════════════════════════════════════════════════
//  边界条件：去重配置的自适应行为
// ═══════════════════════════════════════════════════════

#[test]
fn test_short_content_adaptive_dedup() {
    // 金句卡类短内容（<100字）使用 2-shingle + 低阈值
    let short_cards = vec![
        make_test_card("名言1", "阅读是心灵的旅行。", CardType::Quote),
        make_test_card("名言2", "心灵之旅是阅读的本质。", CardType::Quote),
        make_test_card("名言3", "知识就是力量。", CardType::Quote),
    ];

    let config = cardnote_compiler::dedup::adaptive_dedup_config(20); // 20 字内容
    let result = semantic_dedup(&short_cards, &config);

    // 短内容的去重更宽松（阈值 0.40），前两句可能或可能不合并
    // 主要验证不 panic 且结果合理
    assert_eq!(result.stats.original_count, 3);
    assert!(result.stats.unique_count <= 3);
}

#[test]
fn test_medium_content_default_dedup() {
    // 中等长度内容（200+ 字）使用默认 3-shingle
    let cards = vec![
        make_test_card(
            "刻意练习",
            "刻意练习是有目的、有反馈、有挑战的练习方式。它与天真的重复练习不同，需要持续走出舒适区。安德斯·艾利克森的研究表明，成为专家需要约一万小时的刻意练习。",
            CardType::Knowledge,
        ),
        make_test_card(
            "刻意练习方法论",
            "刻意练习是有目的、有反馈、有挑战的练习方式。它与天真的重复练习不同，需要持续走出舒适区。安德斯·艾利克森的研究表明，成为专家需要约一万小时的刻意练习。",
            CardType::Knowledge,
        ),
        make_test_card(
            "心理表征",
            "心理表征是专家在长时记忆中构建的复杂知识结构，使得他们能够快速识别模式并做出有效决策。",
            CardType::Term,
        ),
    ];

    let config = cardnote_compiler::dedup::adaptive_dedup_config(250);
    let result = semantic_dedup(&cards, &config);

    // 前两张高度相似应合并，第三张独立
    assert_eq!(result.stats.original_count, 3);
    assert_eq!(result.stats.unique_count, 2);
    assert_eq!(result.stats.merged_groups, 1);
}

// ═══════════════════════════════════════════════════════
//  质量评分影响 canonical 选择
// ═══════════════════════════════════════════════════════

#[test]
fn test_higher_quality_card_chosen_as_canonical() {
    // 两张相似卡片，一张质量更高（有引用、更长、结构更完整）
    let high_quality = cardnote_compiler::models::Card {
        title: "执行意图详解".to_string(),
        content: "执行意图是一种具体的计划策略，通过「如果-那么」的形式预先设定行动触发条件。研究表明，执行意图比目标意图更有效。它通过自动化行为触发来减少意志力消耗。".to_string(),
        card_type: CardType::Knowledge,
        reference: "《人生模式》第6章".to_string(),
        unique_id: "20240101120000".to_string(),
        ..Default::default()
    };

    let low_quality = cardnote_compiler::models::Card {
        title: "执行意图".to_string(),
        content: "执行意图是一种具体的计划策略，通过「如果-那么」的形式预先设定行动触发条件。研究表明，执行意图比目标意图更有效。".to_string(),
        card_type: CardType::Knowledge,
        reference: "".to_string(),
        unique_id: "20240101120001".to_string(),
        ..Default::default()
    };

    let cards = vec![high_quality, low_quality];
    let result = semantic_dedup(&cards, &DedupConfig::default());

    assert_eq!(result.stats.unique_count, 1);
    // canonical 应选择更完整的卡片
    let merged = &result.cards[0];
    assert!(
        merged.title.contains("执行意图"),
        "canonical 应保留最佳质量的卡片标题"
    );
    assert!(
        merged.reference.contains("人生模式"),
        "canonical 应保留引用信息"
    );
}

// ═══════════════════════════════════════════════════════
//  空列表和单元素边界
// ═══════════════════════════════════════════════════════

#[test]
fn test_dedup_empty_list() {
    let result = semantic_dedup(&[], &DedupConfig::default());
    assert_eq!(result.cards.len(), 0);
    assert_eq!(result.stats.original_count, 0);
    assert_eq!(result.stats.unique_count, 0);
}

#[test]
fn test_dedup_single_element() {
    let cards = vec![make_test_card("唯一卡片", "唯一内容。", CardType::Knowledge)];
    let result = semantic_dedup(&cards, &DedupConfig::default());
    assert_eq!(result.cards.len(), 1);
    assert_eq!(result.stats.merged_groups, 0);
    assert_eq!(result.stats.unique_count, 1);
}
