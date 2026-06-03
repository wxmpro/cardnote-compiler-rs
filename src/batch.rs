//! 编译记录追踪 — SQLite 持久化
//!
//! 数据模型设计原则：
//!   1. 一本书对应一个项目（books 表通常只有一条记录，但支持多本书历史）
//!   2. 书内卡片去重（cards.unique_id 在单书内唯一）
//!   3. 跨书实体不合并（entities 隔离在 compilation_id 下）
//!   4. 字段尽可能全：覆盖所有 Rust 模型字段
//!   5. 外键强制 + 事务包裹 + 错误不静默

use std::path::Path;

use rusqlite::{Connection, Transaction};

use crate::error::{AppError, Result};
use crate::models::{
    Card, CardStatus, ChunkInfo, CompilationDiagnostics, CompilationResult, Entity, Relation,
    Summary,
};

// ═══════════════════════════════════════════════════════
//  Schema 版本管理
// ═══════════════════════════════════════════════════════

const SCHEMA_VERSION: i64 = 2;

/// 数据库 schema 迁移：从旧版本升级到新版本
fn migrate(db: &Connection, from_version: i64) -> Result<()> {
    if from_version >= SCHEMA_VERSION {
        return Ok(());
    }

    eprintln!("  → 数据库 schema 升级: v{} → v{}", from_version, SCHEMA_VERSION);

    // v1 → v2: 完整模型重构（新增 summaries/chunks/relations/diagnostics 等表）
    if from_version < 2 {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS summaries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                compilation_id INTEGER NOT NULL REFERENCES compilations(id) ON DELETE CASCADE,
                title TEXT NOT NULL DEFAULT '',
                overview TEXT NOT NULL DEFAULT '',
                structure TEXT NOT NULL DEFAULT '',
                key_points_json TEXT NOT NULL DEFAULT '[]'
            );

             CREATE TABLE IF NOT EXISTS chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                compilation_id INTEGER NOT NULL REFERENCES compilations(id) ON DELETE CASCADE,
                chunk_index INTEGER NOT NULL DEFAULT 0,
                title_path TEXT NOT NULL DEFAULT '',
                content_preview TEXT NOT NULL DEFAULT '',
                char_count INTEGER NOT NULL DEFAULT 0,
                entity_count INTEGER NOT NULL DEFAULT 0,
                card_count INTEGER NOT NULL DEFAULT 0,
                relation_count INTEGER NOT NULL DEFAULT 0,
                UNIQUE(compilation_id, chunk_index)
            );

             ALTER TABLE cards ADD COLUMN content TEXT NOT NULL DEFAULT '';
             ALTER TABLE cards ADD COLUMN original_text TEXT NOT NULL DEFAULT '';
             ALTER TABLE cards ADD COLUMN source TEXT NOT NULL DEFAULT '';
             ALTER TABLE cards ADD COLUMN paraphrase TEXT NOT NULL DEFAULT '';
             ALTER TABLE cards ADD COLUMN evidence TEXT NOT NULL DEFAULT '';
             ALTER TABLE cards ADD COLUMN location TEXT NOT NULL DEFAULT '';
             ALTER TABLE cards ADD COLUMN source_file TEXT NOT NULL DEFAULT '';
             ALTER TABLE cards ADD COLUMN chunk_id TEXT NOT NULL DEFAULT '';
             ALTER TABLE cards ADD COLUMN related_cards_json TEXT NOT NULL DEFAULT '[]';
             ALTER TABLE cards ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE cards ADD COLUMN degraded_from TEXT;

             ALTER TABLE entities ADD COLUMN context TEXT NOT NULL DEFAULT '';
             ALTER TABLE entities ADD COLUMN aliases_json TEXT NOT NULL DEFAULT '[]';
             ALTER TABLE entities ADD COLUMN mention_count INTEGER NOT NULL DEFAULT 1;
             ALTER TABLE entities ADD COLUMN first_appearance_chunk_id TEXT NOT NULL DEFAULT '';

             CREATE TABLE IF NOT EXISTS relations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                compilation_id INTEGER NOT NULL REFERENCES compilations(id) ON DELETE CASCADE,
                source_name TEXT NOT NULL DEFAULT '',
                target_name TEXT NOT NULL DEFAULT '',
                relation_type TEXT NOT NULL DEFAULT '',
                evidence TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_relations_compilation ON relations(compilation_id);

             CREATE TABLE IF NOT EXISTS card_entities (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                card_id INTEGER NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
                entity_id INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
                UNIQUE(card_id, entity_id)
            );
            CREATE INDEX IF NOT EXISTS idx_card_entities_card ON card_entities(card_id);
            CREATE INDEX IF NOT EXISTS idx_card_entities_entity ON card_entities(entity_id);

             CREATE TABLE IF NOT EXISTS diagnostics (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                compilation_id INTEGER NOT NULL REFERENCES compilations(id) ON DELETE CASCADE,
                diag_type TEXT NOT NULL CHECK(diag_type IN ('failure','degradation','retry')),
                stage TEXT NOT NULL DEFAULT '',
                message TEXT NOT NULL DEFAULT '',
                retry_count INTEGER DEFAULT 0,
                expected_count INTEGER DEFAULT 0,
                actual_count INTEGER DEFAULT 0,
                final_status TEXT DEFAULT '',
                success INTEGER DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_diagnostics_compilation ON diagnostics(compilation_id);

             ALTER TABLE compilations ADD COLUMN provider TEXT NOT NULL DEFAULT '';
             ALTER TABLE compilations ADD COLUMN doc_lines INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE compilations ADD COLUMN chunk_count INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE compilations ADD COLUMN degraded_cards INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE compilations ADD COLUMN retry_cards INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE compilations ADD COLUMN cached_tokens INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE compilations ADD COLUMN cache_creation_tokens INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE compilations ADD COLUMN cost_estimate REAL DEFAULT 0.0;
             ALTER TABLE compilations ADD COLUMN duration_ms INTEGER DEFAULT 0;
             ALTER TABLE compilations ADD COLUMN markdown_path TEXT NOT NULL DEFAULT '';
             ALTER TABLE compilations ADD COLUMN success INTEGER NOT NULL DEFAULT 1;
             ALTER TABLE compilations ADD COLUMN error_message TEXT NOT NULL DEFAULT '';

             ALTER TABLE books ADD COLUMN page_count INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE books ADD COLUMN file_size INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE books ADD COLUMN file_format TEXT NOT NULL DEFAULT '';
             ALTER TABLE books ADD COLUMN content_hash TEXT NOT NULL DEFAULT '';
            ",
        )
        .map_err(|e| AppError::TaskPanic(format!("Schema 升级 v1→v2 失败: {}", e)))?;
    }

    db.execute(
        "INSERT OR REPLACE INTO schema_migrations (version, applied_at) VALUES (?1, datetime('now'))",
        [SCHEMA_VERSION],
    )
    .map_err(|e| AppError::TaskPanic(format!("更新 schema 版本失败: {}", e)))?;

    Ok(())
}

// ═══════════════════════════════════════════════════════
//  数据模型
// ═══════════════════════════════════════════════════════

/// 单次编译记录（查询用）
#[derive(Debug, Clone)]
pub struct CompileRecord {
    pub id: i64,
    pub source_file: String,
    pub book_title: String,
    pub version: i64,
    pub strategy: String,
    pub model: String,
    pub doc_chars: i64,
    pub total_cards: i64,
    pub accepted_cards: i64,
    pub rejected_cards: i64,
    pub entity_count: i64,
    pub relation_count: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub output_dir: String,
    pub reviewed: bool,
    pub compiled_at: String,
}

/// 编译统计摘要
#[derive(Debug, Clone, Default)]
pub struct CompileStats {
    pub total_compilations: i64,
    pub unique_books: i64,
    pub total_cards: i64,
    pub total_tokens: i64,
    pub pending_review: i64,
}

/// 编译追踪器
pub struct CompileTracker {
    db: Connection,
}

impl CompileTracker {
    pub fn new() -> Result<Self> {
        let db_path = Path::new(".cardnote/compilations.db");
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let db = Connection::open(db_path).map_err(|e| {
            AppError::TaskPanic(format!("无法打开编译记录数据库: {}", e))
        })?;

        // 强制开启外键约束（SQLite 默认关闭）
        db.execute("PRAGMA foreign_keys = ON", [])
            .map_err(|e| AppError::TaskPanic(format!("无法开启外键约束: {}", e)))?;

        // Schema 版本管理表
        db.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            [],
        )
        .map_err(|e| AppError::TaskPanic(format!("无法创建 schema_migrations 表: {}", e)))?;

        // 获取当前 schema 版本
        let current_version: i64 = db
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // 首次创建或迁移
        if current_version == 0 {
            Self::create_v2_schema(&db)?;
        } else if current_version < SCHEMA_VERSION {
            migrate(&db, current_version)?;
        }

        Ok(Self { db })
    }

    /// 创建 v2 完整 Schema（新数据库）
    fn create_v2_schema(db: &Connection) -> Result<()> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS books (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_file TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL DEFAULT '',
                author TEXT NOT NULL DEFAULT '',
                publisher TEXT NOT NULL DEFAULT '',
                isbn TEXT NOT NULL DEFAULT '',
                page_count INTEGER NOT NULL DEFAULT 0,
                file_size INTEGER NOT NULL DEFAULT 0,
                file_format TEXT NOT NULL DEFAULT '',
                content_hash TEXT NOT NULL DEFAULT '',
                first_compiled_at TEXT NOT NULL DEFAULT (datetime('now')),
                last_compiled_at TEXT NOT NULL DEFAULT (datetime('now')),
                compile_count INTEGER NOT NULL DEFAULT 1,
                last_success INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_books_title ON books(title);
            CREATE INDEX IF NOT EXISTS idx_books_content_hash ON books(content_hash);

             CREATE TABLE IF NOT EXISTS compilations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                book_id INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                version INTEGER NOT NULL DEFAULT 1,
                strategy TEXT NOT NULL DEFAULT '',
                model TEXT NOT NULL DEFAULT '',
                provider TEXT NOT NULL DEFAULT '',
                doc_chars INTEGER NOT NULL DEFAULT 0,
                doc_lines INTEGER NOT NULL DEFAULT 0,
                chunk_count INTEGER NOT NULL DEFAULT 0,
                total_cards INTEGER NOT NULL DEFAULT 0,
                accepted_cards INTEGER NOT NULL DEFAULT 0,
                rejected_cards INTEGER NOT NULL DEFAULT 0,
                degraded_cards INTEGER NOT NULL DEFAULT 0,
                retry_cards INTEGER NOT NULL DEFAULT 0,
                entity_count INTEGER NOT NULL DEFAULT 0,
                relation_count INTEGER NOT NULL DEFAULT 0,
                prompt_tokens INTEGER NOT NULL DEFAULT 0,
                completion_tokens INTEGER NOT NULL DEFAULT 0,
                cached_tokens INTEGER NOT NULL DEFAULT 0,
                cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
                cost_estimate REAL NOT NULL DEFAULT 0.0,
                duration_ms INTEGER NOT NULL DEFAULT 0,
                output_dir TEXT NOT NULL DEFAULT '',
                markdown_path TEXT NOT NULL DEFAULT '',
                reviewed INTEGER NOT NULL DEFAULT 0,
                success INTEGER NOT NULL DEFAULT 1,
                error_message TEXT NOT NULL DEFAULT '',
                compiled_at TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(book_id, version)
            );
            CREATE INDEX IF NOT EXISTS idx_compilations_book ON compilations(book_id);
            CREATE INDEX IF NOT EXISTS idx_compilations_date ON compilations(compiled_at);

             CREATE TABLE IF NOT EXISTS summaries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                compilation_id INTEGER NOT NULL REFERENCES compilations(id) ON DELETE CASCADE,
                title TEXT NOT NULL DEFAULT '',
                overview TEXT NOT NULL DEFAULT '',
                structure TEXT NOT NULL DEFAULT '',
                key_points_json TEXT NOT NULL DEFAULT '[]'
            );
            CREATE INDEX IF NOT EXISTS idx_summaries_compilation ON summaries(compilation_id);

             CREATE TABLE IF NOT EXISTS chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                compilation_id INTEGER NOT NULL REFERENCES compilations(id) ON DELETE CASCADE,
                chunk_index INTEGER NOT NULL DEFAULT 0,
                title_path TEXT NOT NULL DEFAULT '',
                content_preview TEXT NOT NULL DEFAULT '',
                char_count INTEGER NOT NULL DEFAULT 0,
                entity_count INTEGER NOT NULL DEFAULT 0,
                card_count INTEGER NOT NULL DEFAULT 0,
                relation_count INTEGER NOT NULL DEFAULT 0,
                UNIQUE(compilation_id, chunk_index)
            );
            CREATE INDEX IF NOT EXISTS idx_chunks_compilation ON chunks(compilation_id);

             CREATE TABLE IF NOT EXISTS cards (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                compilation_id INTEGER NOT NULL REFERENCES compilations(id) ON DELETE CASCADE,
                chunk_id TEXT NOT NULL DEFAULT '',
                unique_id TEXT NOT NULL DEFAULT '',
                title TEXT NOT NULL DEFAULT '',
                content TEXT NOT NULL DEFAULT '',
                card_type TEXT NOT NULL DEFAULT 'Knowledge',
                quality_score REAL NOT NULL DEFAULT 1.0,
                status TEXT NOT NULL DEFAULT 'Accepted',
                reject_reason TEXT NOT NULL DEFAULT '',
                retry_count INTEGER NOT NULL DEFAULT 0,
                reference TEXT NOT NULL DEFAULT '',
                source_file TEXT NOT NULL DEFAULT '',
                location TEXT NOT NULL DEFAULT '',
                original_text TEXT NOT NULL DEFAULT '',
                source TEXT NOT NULL DEFAULT '',
                paraphrase TEXT NOT NULL DEFAULT '',
                evidence TEXT NOT NULL DEFAULT '',
                related_cards_json TEXT NOT NULL DEFAULT '[]',
                degraded_from TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_cards_compilation ON cards(compilation_id);
            CREATE INDEX IF NOT EXISTS idx_cards_type ON cards(card_type);
            CREATE INDEX IF NOT EXISTS idx_cards_unique_id ON cards(unique_id);

             CREATE TABLE IF NOT EXISTS entities (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                compilation_id INTEGER NOT NULL REFERENCES compilations(id) ON DELETE CASCADE,
                name TEXT NOT NULL DEFAULT '',
                entity_type TEXT NOT NULL DEFAULT '',
                context TEXT NOT NULL DEFAULT '',
                aliases_json TEXT NOT NULL DEFAULT '[]',
                mention_count INTEGER NOT NULL DEFAULT 1,
                first_appearance_chunk_id TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_entities_compilation ON entities(compilation_id);
            CREATE INDEX IF NOT EXISTS idx_entities_name ON entities(name);

             CREATE TABLE IF NOT EXISTS relations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                compilation_id INTEGER NOT NULL REFERENCES compilations(id) ON DELETE CASCADE,
                source_name TEXT NOT NULL DEFAULT '',
                target_name TEXT NOT NULL DEFAULT '',
                relation_type TEXT NOT NULL DEFAULT '',
                evidence TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_relations_compilation ON relations(compilation_id);
            CREATE INDEX IF NOT EXISTS idx_relations_source ON relations(source_name);
            CREATE INDEX IF NOT EXISTS idx_relations_target ON relations(target_name);

             CREATE TABLE IF NOT EXISTS card_entities (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                card_id INTEGER NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
                entity_id INTEGER NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
                UNIQUE(card_id, entity_id)
            );
            CREATE INDEX IF NOT EXISTS idx_card_entities_card ON card_entities(card_id);
            CREATE INDEX IF NOT EXISTS idx_card_entities_entity ON card_entities(entity_id);

             CREATE TABLE IF NOT EXISTS diagnostics (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                compilation_id INTEGER NOT NULL REFERENCES compilations(id) ON DELETE CASCADE,
                diag_type TEXT NOT NULL CHECK(diag_type IN ('failure','degradation','retry')),
                stage TEXT NOT NULL DEFAULT '',
                message TEXT NOT NULL DEFAULT '',
                retry_count INTEGER NOT NULL DEFAULT 0,
                expected_count INTEGER NOT NULL DEFAULT 0,
                actual_count INTEGER NOT NULL DEFAULT 0,
                final_status TEXT NOT NULL DEFAULT '',
                success INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_diagnostics_compilation ON diagnostics(compilation_id);

             INSERT INTO schema_migrations (version) VALUES (2);
            ",
        )
        .map_err(|e| AppError::TaskPanic(format!("无法创建编译记录表: {}", e)))?;

        Ok(())
    }

    // ── Books ──

    /// 确保书在 books 表中存在。返回 book_id
    pub fn ensure_book(
        &self,
        source_file: &str,
        title: &str,
        author: &str,
        publisher: &str,
        isbn: &str,
        page_count: usize,
        file_size: u64,
        file_format: &str,
        content_hash: &str,
    ) -> Result<i64> {
        self.db
            .execute(
                "INSERT INTO books (source_file, title, author, publisher, isbn, page_count, file_size, file_format, content_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(source_file) DO UPDATE SET
                    title = CASE WHEN excluded.title <> '' AND (books.title = '' OR books.title = '未命名') THEN excluded.title ELSE books.title END,
                    author = CASE WHEN excluded.author <> '' AND books.author = '' THEN excluded.author ELSE books.author END,
                    publisher = CASE WHEN excluded.publisher <> '' AND books.publisher = '' THEN excluded.publisher ELSE books.publisher END,
                    isbn = CASE WHEN excluded.isbn <> '' AND books.isbn = '' THEN excluded.isbn ELSE books.isbn END,
                    page_count = CASE WHEN excluded.page_count > 0 AND books.page_count = 0 THEN excluded.page_count ELSE books.page_count END,
                    content_hash = CASE WHEN excluded.content_hash <> '' THEN excluded.content_hash ELSE books.content_hash END,
                    compile_count = compile_count + 1,
                    last_compiled_at = datetime('now')",
                rusqlite::params![
                    source_file, title, author, publisher, isbn,
                    page_count as i64, file_size as i64, file_format, content_hash
                ],
            )
            .map_err(|e| AppError::TaskPanic(format!("确保书籍记录失败: {}", e)))?;

        let book_id: i64 = self
            .db
            .query_row(
                "SELECT id FROM books WHERE source_file = ?1",
                [source_file],
                |row| row.get(0),
            )
            .map_err(|e| AppError::TaskPanic(format!("查询书籍ID失败: {}", e)))?;

        Ok(book_id)
    }

    pub fn update_book_status(&self, book_id: i64, success: bool) -> Result<()> {
        self.db.execute(
            "UPDATE books SET last_success = ?1, last_compiled_at = datetime('now') WHERE id = ?2",
            rusqlite::params![success as i64, book_id],
        ).map_err(|e| AppError::TaskPanic(format!("更新书籍状态失败: {}", e)))?;
        Ok(())
    }

    // ── Compilations ──

    /// 记录一次完整编译（事务包裹所有子表写入）
    pub fn record_compilation(
        &self,
        book_id: i64,
        result: &CompilationResult,
        cfg: &RecordConfig,
    ) -> Result<i64> {
        let tx = self.db.unchecked_transaction()
            .map_err(|e| AppError::TaskPanic(format!("开启事务失败: {}", e)))?;

        let compilation_id = Self::insert_compilation(&tx, book_id, result, cfg)?;

        // 写入摘要
        if !result.summary.title.is_empty() || !result.summary.overview.is_empty() {
            Self::insert_summary(&tx, compilation_id, &result.summary)?;
        }

        // 写入分块信息
        for (idx, chunk) in result.chunks.iter().enumerate() {
            Self::insert_chunk(&tx, compilation_id, idx, chunk)?;
        }

        // 写入卡片（事务内批量插入）
        let accepted = result.cards.iter().filter(|c| c.status == CardStatus::Accepted && c.reject_reason.is_empty()).count();
        let rejected = result.cards.len() - accepted;
        let degraded = result.cards.iter().filter(|c| c.status == CardStatus::Degraded).count();
        let retry = result.cards.iter().filter(|c| c.status == CardStatus::NeedsRetry).count();

        for card in &result.cards {
            Self::insert_card(&tx, compilation_id, card)?;
        }

        // 写入实体
        for entity in &result.graph.entities {
            Self::insert_entity(&tx, compilation_id, entity)?;
        }

        // 写入关系
        for relation in &result.graph.relations {
            Self::insert_relation(&tx, compilation_id, relation)?;
        }

        // 写入诊断信息
        Self::insert_diagnostics(&tx, compilation_id, &result.diagnostics)?;

        // 更新 compilations 统计（以实际写入为准）
        tx.execute(
            "UPDATE compilations SET
                total_cards = ?1,
                accepted_cards = ?2,
                rejected_cards = ?3,
                degraded_cards = ?4,
                retry_cards = ?5,
                entity_count = ?6,
                relation_count = ?7
             WHERE id = ?8",
            rusqlite::params![
                result.cards.len() as i64,
                accepted as i64,
                rejected as i64,
                degraded as i64,
                retry as i64,
                result.graph.entities.len() as i64,
                result.graph.relations.len() as i64,
                compilation_id,
            ],
        ).map_err(|e| AppError::TaskPanic(format!("更新编译统计失败: {}", e)))?;

        tx.commit()
            .map_err(|e| AppError::TaskPanic(format!("提交编译记录事务失败: {}", e)))?;

        Ok(compilation_id)
    }

    fn insert_compilation(
        tx: &Transaction,
        book_id: i64,
        _result: &CompilationResult,
        cfg: &RecordConfig,
    ) -> Result<i64> {
        let max_version: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM compilations WHERE book_id = ?1",
                [book_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let version = max_version + 1;

        tx.execute(
            "INSERT INTO compilations
             (book_id, version, strategy, model, provider, doc_chars,
              total_cards, accepted_cards, rejected_cards,
              entity_count, relation_count,
              prompt_tokens, completion_tokens,
              output_dir, markdown_path, duration_ms, success, error_message)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
            rusqlite::params![
                book_id, version, cfg.strategy, cfg.model, cfg.provider,
                cfg.doc_chars as i64,
                cfg.total_cards as i64,
                cfg.accepted_cards as i64,
                cfg.rejected_cards as i64,
                cfg.entity_count as i64,
                cfg.relation_count as i64,
                cfg.prompt_tokens as i64,
                cfg.completion_tokens as i64,
                cfg.output_dir,
                cfg.markdown_path,
                cfg.duration_ms as i64,
                cfg.success as i64,
                cfg.error_message,
            ],
        )
        .map_err(|e| AppError::TaskPanic(format!("插入编译记录失败: {}", e)))?;

        Ok(tx.last_insert_rowid())
    }

    fn insert_summary(tx: &Transaction, compilation_id: i64, summary: &Summary) -> Result<()> {
        let key_points_json = serde_json::to_string(&summary.key_points)
            .unwrap_or_else(|_| "[]".to_string());

        tx.execute(
            "INSERT INTO summaries (compilation_id, title, overview, structure, key_points_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                compilation_id,
                summary.title,
                summary.overview,
                summary.structure,
                key_points_json,
            ],
        )
        .map_err(|e| AppError::TaskPanic(format!("插入摘要失败: {}", e)))?;
        Ok(())
    }

    fn insert_chunk(
        tx: &Transaction,
        compilation_id: i64,
        chunk_index: usize,
        chunk: &ChunkInfo,
    ) -> Result<()> {
        tx.execute(
            "INSERT INTO chunks (compilation_id, chunk_index, title_path, content_preview, char_count, entity_count, card_count, relation_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                compilation_id,
                chunk_index as i64,
                chunk.title_path,
                "", // content_preview 需要额外传入，这里留空
                chunk.size as i64,
                chunk.entities as i64,
                chunk.cards as i64,
                chunk.relations as i64,
            ],
        )
        .map_err(|e| AppError::TaskPanic(format!("插入分块信息失败: {}", e)))?;
        Ok(())
    }

    fn insert_card(tx: &Transaction, compilation_id: i64, card: &Card) -> Result<()> {
        let related_cards_json = serde_json::to_string(&card.related_cards)
            .unwrap_or_else(|_| "[]".to_string());
        let degraded_from = card.degraded_from.as_ref().map(|t| t.to_string());

        tx.execute(
            "INSERT INTO cards
             (compilation_id, chunk_id, unique_id, title, content, card_type,
              quality_score, status, reject_reason, retry_count,
              reference, source_file, location,
              original_text, source, paraphrase, evidence,
              related_cards_json, degraded_from)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
            rusqlite::params![
                compilation_id,
                card.chunk_id,
                card.unique_id,
                card.title,
                card.content,
                format!("{:?}", card.card_type),
                card.quality_score,
                format!("{:?}", card.status),
                card.reject_reason,
                card.retry_count as i64,
                card.reference,
                card.source_file,
                card.location,
                card.original_text,
                card.source,
                card.paraphrase,
                card.evidence,
                related_cards_json,
                degraded_from,
            ],
        )
        .map_err(|e| AppError::TaskPanic(format!("插入卡片失败: {}", e)))?;
        Ok(())
    }

    fn insert_entity(tx: &Transaction, compilation_id: i64, entity: &Entity) -> Result<()> {
        tx.execute(
            "INSERT INTO entities (compilation_id, name, entity_type, context)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                compilation_id,
                entity.name,
                entity.entity_type,
                entity.context,
            ],
        )
        .map_err(|e| AppError::TaskPanic(format!("插入实体失败: {}", e)))?;
        Ok(())
    }

    fn insert_relation(tx: &Transaction, compilation_id: i64, relation: &Relation) -> Result<()> {
        tx.execute(
            "INSERT INTO relations (compilation_id, source_name, target_name, relation_type, evidence)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                compilation_id,
                relation.source,
                relation.target,
                relation.relation_type,
                relation.evidence,
            ],
        )
        .map_err(|e| AppError::TaskPanic(format!("插入关系失败: {}", e)))?;
        Ok(())
    }

    fn insert_diagnostics(
        tx: &Transaction,
        compilation_id: i64,
        diagnostics: &CompilationDiagnostics,
    ) -> Result<()> {
        for fail in &diagnostics.failures {
            tx.execute(
                "INSERT INTO diagnostics (compilation_id, diag_type, stage, message, retry_count, final_status)
                 VALUES (?1, 'failure', ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    compilation_id,
                    fail.stage,
                    fail.error,
                    fail.retry_count,
                    fail.final_status,
                ],
            )
            .map_err(|e| AppError::TaskPanic(format!("插入诊断失败记录失败: {}", e)))?;
        }

        for deg in &diagnostics.degradations {
            tx.execute(
                "INSERT INTO diagnostics (compilation_id, diag_type, stage, message, expected_count, actual_count)
                 VALUES (?1, 'degradation', ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    compilation_id,
                    deg.stage,
                    deg.reason,
                    deg.expected_count as i64,
                    deg.actual_count as i64,
                ],
            )
            .map_err(|e| AppError::TaskPanic(format!("插入诊断降级记录失败: {}", e)))?;
        }

        for retry in &diagnostics.retries {
            tx.execute(
                "INSERT INTO diagnostics (compilation_id, diag_type, stage, retry_count, success)
                 VALUES (?1, 'retry', ?2, ?3, ?4)",
                rusqlite::params![
                    compilation_id,
                    retry.stage,
                    retry.retry_count,
                    retry.success as i64,
                ],
            )
            .map_err(|e| AppError::TaskPanic(format!("插入诊断重试记录失败: {}", e)))?;
        }

        Ok(())
    }

    // ── Queries ──

    /// 获取最近 N 条编译记录
    pub fn recent(&self, limit: usize) -> Result<Vec<CompileRecord>> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT c.id, b.source_file, b.title, c.version, c.strategy, c.model, c.doc_chars,
                        c.total_cards, c.accepted_cards, c.rejected_cards,
                        c.entity_count, c.relation_count,
                        c.prompt_tokens, c.completion_tokens,
                        c.output_dir, c.reviewed, c.compiled_at
                 FROM compilations c JOIN books b ON c.book_id = b.id
                 ORDER BY c.compiled_at DESC LIMIT ?1",
            )
            .map_err(|e| AppError::TaskPanic(format!("查询编译记录失败: {}", e)))?;

        let rows = stmt
            .query_map([limit as i64], Self::map_row)
            .map_err(|e| AppError::TaskPanic(format!("查询编译记录失败: {}", e)))?;

        let mut records = Vec::new();
        for row in rows {
            match row {
                Ok(r) => records.push(r),
                Err(e) => eprintln!("  ⚠ 解析编译记录行失败: {}", e),
            }
        }
        Ok(records)
    }

    /// 获取某次编译的卡片统计
    pub fn card_stats(&self, compilation_id: i64) -> Result<Vec<(String, i64)>> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT card_type, COUNT(*) FROM cards WHERE compilation_id = ?1 GROUP BY card_type ORDER BY COUNT(*) DESC",
            )
            .map_err(|e| AppError::TaskPanic(format!("查询卡片统计失败: {}", e)))?;

        let rows = stmt
            .query_map([compilation_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| AppError::TaskPanic(format!("查询卡片统计失败: {}", e)))?;

        let mut result = Vec::new();
        for row in rows {
            match row {
                Ok(r) => result.push(r),
                Err(e) => eprintln!("  ⚠ 解析卡片统计行失败: {}", e),
            }
        }
        Ok(result)
    }

    /// 标记编译记录为已审阅
    pub fn mark_reviewed(&self, id: i64) -> Result<()> {
        self.db
            .execute("UPDATE compilations SET reviewed = 1 WHERE id = ?1", [id])
            .map_err(|e| AppError::TaskPanic(format!("标记审阅失败: {}", e)))?;
        Ok(())
    }

    /// 获取编译统计
    pub fn stats(&self) -> Result<CompileStats> {
        let total: i64 = self
            .db
            .query_row("SELECT COUNT(*) FROM compilations", [], |row| row.get(0))
            .unwrap_or(0);
        let unique_books: i64 = self
            .db
            .query_row("SELECT COUNT(*) FROM books", [], |row| row.get(0))
            .unwrap_or(0);
        let total_cards: i64 = self
            .db
            .query_row(
                "SELECT COALESCE(SUM(accepted_cards), 0) FROM compilations",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let total_tokens: i64 = self
            .db
            .query_row(
                "SELECT COALESCE(SUM(prompt_tokens + completion_tokens), 0) FROM compilations",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let pending_review: i64 = self
            .db
            .query_row(
                "SELECT COUNT(*) FROM compilations WHERE reviewed = 0",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(CompileStats {
            total_compilations: total,
            unique_books,
            total_cards,
            total_tokens,
            pending_review,
        })
    }

    fn map_row(row: &rusqlite::Row<'_>) -> std::result::Result<CompileRecord, rusqlite::Error> {
        Ok(CompileRecord {
            id: row.get(0)?,
            source_file: row.get(1)?,
            book_title: row.get(2)?,
            version: row.get(3)?,
            strategy: row.get(4)?,
            model: row.get(5)?,
            doc_chars: row.get(6)?,
            total_cards: row.get(7)?,
            accepted_cards: row.get(8)?,
            rejected_cards: row.get(9)?,
            entity_count: row.get(10)?,
            relation_count: row.get(11)?,
            prompt_tokens: row.get(12)?,
            completion_tokens: row.get(13)?,
            output_dir: row.get(14)?,
            reviewed: row.get::<_, i64>(15)? != 0,
            compiled_at: row.get(16)?,
        })
    }
}

// ═══════════════════════════════════════════════════════
//  编译记录配置（传入 record_compilation）
// ═══════════════════════════════════════════════════════

/// 编译记录的配置参数
#[derive(Debug, Clone, Default)]
pub struct RecordConfig {
    pub strategy: String,
    pub model: String,
    pub provider: String,
    pub doc_chars: usize,
    pub total_cards: usize,
    pub accepted_cards: usize,
    pub rejected_cards: usize,
    pub entity_count: usize,
    pub relation_count: usize,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub output_dir: String,
    pub markdown_path: String,
    pub duration_ms: u64,
    pub success: bool,
    pub error_message: String,
}
