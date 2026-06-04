//! SQLite 持久化集成测试 — 验证编译记录的完整读写链路
//!
//! 注意: CompileTracker 使用固定的 `.cardnote/compilations.db` 相对路径。
//! 测试通过切换工作目录到临时目录来隔离数据库。
//! 由于 `set_current_dir` 是进程级全局操作，这些测试串行运行。

mod common;

use std::sync::Mutex;

use cardnote_compiler::batch::{CompileTracker, RecordConfig};
use cardnote_compiler::models::{CardStatus, CardType};
use common::{make_test_card, make_test_compilation_result};

/// 全局锁：确保 SQLite 测试串行运行（set_current_dir 是进程全局的）
static SQLITE_TEST_LOCK: Mutex<()> = Mutex::new(());

/// 在临时目录中创建 CompileTracker（确保测试隔离）
/// 返回 tracker 和保持临时目录存活的 TempDir
fn with_temp_db() -> (CompileTracker, tempfile::TempDir) {
    // 获取全局锁以串行化
    let _guard = SQLITE_TEST_LOCK.lock().unwrap();

    let tmp = tempfile::tempdir().expect("创建临时目录失败");
    std::fs::create_dir_all(tmp.path().join(".cardnote")).unwrap();

    // 切换工作目录到临时目录，使 CompileTracker 使用独立数据库
    let original = std::env::current_dir().expect("获取当前目录失败");
    std::env::set_current_dir(tmp.path()).expect("切换工作目录失败");

    let tracker = CompileTracker::new().expect("创建 CompileTracker 失败");

    // 恢复工作目录。SQLite 已打开文件句柄，切换回原目录不影响读写。
    std::env::set_current_dir(&original).expect("恢复工作目录失败");

    // 注意：_guard (MutexGuard) 在此函数返回时 drop，释放锁。
    // 但 tmp (TempDir) 仍在作用域内，由调用方持有。
    (tracker, tmp)
}

/// 辅助函数：创建最小可用的 RecordConfig
fn make_test_config() -> RecordConfig {
    RecordConfig {
        strategy: "extract_then_assign".to_string(),
        model: "test-model".to_string(),
        provider: "test-provider".to_string(),
        doc_chars: 5000,
        total_cards: 3,
        accepted_cards: 3,
        rejected_cards: 0,
        entity_count: 4,
        relation_count: 2,
        prompt_tokens: 500,
        completion_tokens: 300,
        output_dir: "/tmp/test-output".to_string(),
        markdown_path: "/tmp/test-output/README.md".to_string(),
        duration_ms: 5000,
        success: true,
        error_message: String::new(),
    }
}

// ═══════════════════════════════════════════════════════
//  Book 管理测试
// ═══════════════════════════════════════════════════════

#[test]
fn test_ensure_book_insert_and_update() {
    let (tracker, _tmp) = with_temp_db();

    // 首次插入
    let book_id = tracker
        .ensure_book(
            "/tmp/test-book.pdf",
            "测试书籍",
            "测试作者",
            "",
            "",
            0,
            1024000,
            "pdf",
            "abc123hash",
        )
        .expect("ensure_book 应成功");
    assert!(book_id > 0, "book_id 应为正整数");

    // 再次调用（UPSERT：同一文件路径）
    let book_id2 = tracker
        .ensure_book(
            "/tmp/test-book.pdf",
            "测试书籍（更新标题）",
            "新作者",
            "",
            "",
            0,
            2048000,
            "pdf",
            "def456hash",
        )
        .expect("第二次 ensure_book 应成功");

    // 同一文件应返回相同 book_id（ON CONFLICT DO UPDATE）
    assert_eq!(book_id, book_id2, "UPSERT 应保持 book_id 不变");
}

#[test]
fn test_ensure_book_different_files_different_ids() {
    let (tracker, _tmp) = with_temp_db();

    let id1 = tracker
        .ensure_book("/tmp/book-a.pdf", "书籍A", "", "", "", 0, 1000, "pdf", "hashA")
        .unwrap();
    let id2 = tracker
        .ensure_book("/tmp/book-b.pdf", "书籍B", "", "", "", 0, 2000, "pdf", "hashB")
        .unwrap();

    assert_ne!(id1, id2, "不同文件应有不同 book_id");
}

// ═══════════════════════════════════════════════════════
//  编译记录读写往返测试
// ═══════════════════════════════════════════════════════

#[test]
fn test_record_compilation_and_retrieve() {
    let (tracker, _tmp) = with_temp_db();

    // 1. 注册书籍
    let book_id = tracker
        .ensure_book(
            "/tmp/认知心理学.pdf",
            "认知心理学",
            "艾森克",
            "人民邮电出版社",
            "978-7-115-xxxxx",
            450,
            15728640,
            "pdf",
            "hash123456",
        )
        .unwrap();

    // 2. 创建编译结果
    let mut result = make_test_compilation_result();
    result.summary.title = "认知心理学".to_string();
    result.cards = vec![
        make_test_card("工作记忆", "工作记忆是一个容量有限的...", CardType::Term),
        make_test_card("注意的选择性", "选择性注意是指...", CardType::Knowledge),
    ];
    result.graph.entities = vec![common::make_test_entity("工作记忆", "概念")];
    result.graph.relations = vec![common::make_test_relation("工作记忆", "注意", "关联")];

    // 3. 记录编译
    let mut config = make_test_config();
    config.total_cards = 2;
    config.accepted_cards = 2;
    let compilation_id = tracker
        .record_compilation(book_id, &result, &config)
        .expect("记录编译应成功");
    assert!(compilation_id > 0, "compilation_id 应为正整数");

    // 4. 更新书籍状态
    tracker
        .update_book_status(book_id, true)
        .expect("更新状态应成功");

    // 5. 读取最近记录
    let records = tracker.recent(10).expect("读取最近记录应成功");
    assert!(!records.is_empty(), "应有至少一条记录");

    let record = records
        .iter()
        .find(|r| r.book_title == "认知心理学")
        .expect("应找到认知心理学的记录");

    assert_eq!(record.version, 1);
    assert_eq!(record.accepted_cards, 2);
    assert_eq!(record.rejected_cards, 0);
    assert_eq!(record.book_title, "认知心理学");
}

#[test]
fn test_record_compilation_with_rejected_cards() {
    let (tracker, _tmp) = with_temp_db();

    let book_id = tracker
        .ensure_book("/tmp/test.pdf", "测试", "", "", "", 0, 1000, "pdf", "hash")
        .unwrap();

    let mut result = make_test_compilation_result();
    result.cards = vec![
        make_test_card("通过卡", "内容够长够好...", CardType::Knowledge),
        {
            let mut rejected = make_test_card("被拒卡", "短", CardType::Knowledge);
            rejected.status = CardStatus::Rejected;
            rejected.reject_reason = "内容过短".to_string();
            rejected
        },
    ];

    let mut config = make_test_config();
    config.total_cards = 2;
    config.accepted_cards = 1;
    config.rejected_cards = 1;

    let compilation_id = tracker
        .record_compilation(book_id, &result, &config)
        .expect("记录应成功");

    let records = tracker.recent(10).unwrap();
    let record = records.iter().find(|r| r.book_title == "测试").unwrap();
    assert_eq!(record.accepted_cards, 1);
    assert_eq!(record.rejected_cards, 1);
}

// ═══════════════════════════════════════════════════════
//  统计查询测试
// ═══════════════════════════════════════════════════════

#[test]
fn test_stats_aggregation() {
    let (tracker, _tmp) = with_temp_db();

    // 注册两本书
    let id1 = tracker
        .ensure_book("/tmp/book1.pdf", "书1", "", "", "", 0, 1000, "pdf", "h1")
        .unwrap();
    let id2 = tracker
        .ensure_book("/tmp/book2.pdf", "书2", "", "", "", 0, 1000, "pdf", "h2")
        .unwrap();

    // 记录两次编译
    let mut r1 = make_test_compilation_result();
    r1.cards = vec![make_test_card("卡A", "内容A...", CardType::Knowledge)];
    let c1 = make_test_config();
    tracker.record_compilation(id1, &r1, &c1).unwrap();

    let mut r2 = make_test_compilation_result();
    r2.cards = vec![
        make_test_card("卡B", "内容B...", CardType::Term),
        make_test_card("卡C", "内容C...", CardType::Knowledge),
    ];
    let mut c2 = make_test_config();
    c2.total_cards = 2;
    c2.accepted_cards = 2;
    tracker.record_compilation(id2, &r2, &c2).unwrap();

    // 验证统计
    let stats = tracker.stats().expect("stats 应成功");
    assert_eq!(stats.unique_books, 2, "应有 2 本唯一书籍");
    assert_eq!(stats.total_compilations, 2, "应有 2 次编译");
    assert_eq!(stats.total_cards, 3, "应有 3 张卡片");
}

#[test]
fn test_mark_reviewed() {
    let (tracker, _tmp) = with_temp_db();

    let book_id = tracker
        .ensure_book("/tmp/review-test.pdf", "审阅测试", "", "", "", 0, 1000, "pdf", "h")
        .unwrap();

    let result = make_test_compilation_result();
    let config = make_test_config();
    let compilation_id = tracker
        .record_compilation(book_id, &result, &config)
        .unwrap();

    // 标记为已审阅
    tracker.mark_reviewed(compilation_id).expect("mark_reviewed 应成功");

    let records = tracker.recent(10).unwrap();
    let record = records
        .iter()
        .find(|r| r.book_title == "审阅测试")
        .unwrap();
    assert!(record.reviewed, "标记后 reviewed 应为 true");
}

// ═══════════════════════════════════════════════════════
//  跨书隔离验证
// ═══════════════════════════════════════════════════════

#[test]
fn test_cross_book_isolation_in_sqlite() {
    let (tracker, _tmp) = with_temp_db();

    // Book A: "系统思维" — 认知科学视角
    let id_a = tracker
        .ensure_book("/tmp/系统思维-认知科学.pdf", "系统思维", "", "", "", 0, 1000, "pdf", "hA")
        .unwrap();
    let mut result_a = make_test_compilation_result();
    result_a.cards = vec![make_test_card(
        "系统思维",
        "系统思维是一种从整体出发的思考方式。",
        CardType::Knowledge,
    )];
    let config_a = make_test_config();
    tracker.record_compilation(id_a, &result_a, &config_a).unwrap();

    // Book B: "系统思维" — 管理学视角
    let id_b = tracker
        .ensure_book("/tmp/系统思维-管理学.pdf", "系统思维", "", "", "", 0, 1000, "pdf", "hB")
        .unwrap();
    let mut result_b = make_test_compilation_result();
    result_b.cards = vec![make_test_card(
        "系统思维",
        "系统思维在管理学中强调整体性和协同效应。",
        CardType::Knowledge,
    )];
    let config_b = make_test_config();
    tracker.record_compilation(id_b, &result_b, &config_b).unwrap();

    // 验证两本书的记录独立
    let stats = tracker.stats().unwrap();
    assert_eq!(stats.unique_books, 2);
    assert_eq!(stats.total_compilations, 2);

    let records = tracker.recent(10).unwrap();
    // Book A 和 Book B 的编译记录应各自独立存在
    let book_a_record = records.iter().find(|r| r.book_title == "系统思维" && r.output_dir.contains("系统思维-认知科学")).or_else(|| records.iter().find(|r| r.book_title == "系统思维"));
    let book_b_record = records.iter().find(|r| r.book_title == "系统思维" && r.output_dir.contains("系统思维-管理学")).or_else(|| records.iter().skip(1).find(|r| r.book_title == "系统思维"));
    assert!(book_a_record.is_some(), "Book A 应有记录");
    assert!(book_b_record.is_some(), "Book B 应有记录");
}

// ═══════════════════════════════════════════════════════
//  空结果和边界条件
// ═══════════════════════════════════════════════════════

#[test]
fn test_record_compilation_with_zero_cards() {
    let (tracker, _tmp) = with_temp_db();

    let book_id = tracker
        .ensure_book("/tmp/empty-book.pdf", "空书", "", "", "", 0, 1000, "pdf", "h")
        .unwrap();

    let mut result = make_test_compilation_result();
    result.cards.clear();
    result.graph.entities.clear();
    result.graph.relations.clear();

    let mut config = make_test_config();
    config.total_cards = 0;
    config.accepted_cards = 0;
    config.success = false;
    config.error_message = "LLM 调用全部失败".to_string();

    let _compilation_id = tracker
        .record_compilation(book_id, &result, &config)
        .expect("空卡片编译记录应成功");

    let records = tracker.recent(10).unwrap();
    let record = records.iter().find(|r| r.book_title == "空书").unwrap();
    assert_eq!(record.accepted_cards, 0);
    assert_eq!(record.total_cards, 0);
}

#[test]
fn test_stats_empty_database() {
    let (tracker, _tmp) = with_temp_db();
    let stats = tracker.stats().expect("空数据库 stats 应成功");
    assert_eq!(stats.unique_books, 0);
    assert_eq!(stats.total_compilations, 0);
    assert_eq!(stats.total_cards, 0);
    assert_eq!(stats.pending_review, 0);
}

#[test]
fn test_recent_empty_database() {
    let (tracker, _tmp) = with_temp_db();
    let records = tracker.recent(10).expect("空数据库 recent 应成功");
    assert!(records.is_empty());
}
