//! 编译记录追踪 — SQLite 持久化，同步记录，版本自增
//!
//! Craftsman 模式：每本书单独编译，同文件重复编译自动版本号 +1。

use std::path::Path;

use rusqlite::Connection;

use crate::error::Result;

/// 单次编译记录
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
            crate::error::AppError::TaskPanic(format!("无法打开编译记录数据库: {}", e))
        })?;

        db.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS compilations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_file TEXT NOT NULL,
                book_title TEXT NOT NULL DEFAULT '',
                version INTEGER NOT NULL DEFAULT 1,
                strategy TEXT DEFAULT '',
                model TEXT DEFAULT '',
                doc_chars INTEGER DEFAULT 0,
                total_cards INTEGER DEFAULT 0,
                accepted_cards INTEGER DEFAULT 0,
                rejected_cards INTEGER DEFAULT 0,
                entity_count INTEGER DEFAULT 0,
                relation_count INTEGER DEFAULT 0,
                prompt_tokens INTEGER DEFAULT 0,
                completion_tokens INTEGER DEFAULT 0,
                output_dir TEXT DEFAULT '',
                reviewed INTEGER NOT NULL DEFAULT 0,
                compiled_at TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(source_file, version)
            );
            CREATE INDEX IF NOT EXISTS idx_compilations_file ON compilations(source_file);
            CREATE INDEX IF NOT EXISTS idx_compilations_date ON compilations(compiled_at);",
        )
        .map_err(|e| crate::error::AppError::TaskPanic(format!("无法创建编译记录表: {}", e)))?;

        Ok(Self { db })
    }

    /// 记录一次编译。同文件重复编译时版本号自动 +1（同步处理）
    pub fn record(
        &self,
        source_file: &str,
        book_title: &str,
        strategy: &str,
        model: &str,
        doc_chars: usize,
        total_cards: usize,
        accepted_cards: usize,
        rejected_cards: usize,
        entity_count: usize,
        relation_count: usize,
        prompt_tokens: u32,
        completion_tokens: u32,
        output_dir: &str,
    ) -> Result<i64> {
        // 查询该文件已有的最大版本号
        let max_version: i64 = self
            .db
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM compilations WHERE source_file = ?1",
                [source_file],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let version = max_version + 1;

        self.db
            .execute(
                "INSERT INTO compilations
                 (source_file, book_title, version, strategy, model, doc_chars,
                  total_cards, accepted_cards, rejected_cards,
                  entity_count, relation_count,
                  prompt_tokens, completion_tokens, output_dir)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                rusqlite::params![
                    source_file,
                    book_title,
                    version,
                    strategy,
                    model,
                    doc_chars as i64,
                    total_cards as i64,
                    accepted_cards as i64,
                    rejected_cards as i64,
                    entity_count as i64,
                    relation_count as i64,
                    prompt_tokens as i64,
                    completion_tokens as i64,
                    output_dir,
                ],
            )
            .map_err(|e| crate::error::AppError::TaskPanic(format!("记录编译结果失败: {}", e)))?;

        Ok(version)
    }

    /// 获取最近 N 条编译记录
    pub fn recent(&self, limit: usize) -> Result<Vec<CompileRecord>> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT id, source_file, book_title, version, strategy, model, doc_chars,
                        total_cards, accepted_cards, rejected_cards,
                        entity_count, relation_count,
                        prompt_tokens, completion_tokens,
                        output_dir, reviewed, compiled_at
                 FROM compilations ORDER BY compiled_at DESC LIMIT ?1",
            )
            .map_err(|e| crate::error::AppError::TaskPanic(format!("查询编译记录失败: {}", e)))?;

        Ok(stmt
            .query_map([limit as i64], Self::map_row)
            .map_err(|e| crate::error::AppError::TaskPanic(format!("查询编译记录失败: {}", e)))?
            .filter_map(|r| r.ok())
            .collect())
    }

    /// 标记编译记录为已审阅
    pub fn mark_reviewed(&self, id: i64) -> Result<()> {
        self.db
            .execute("UPDATE compilations SET reviewed = 1 WHERE id = ?1", [id])
            .map_err(|e| crate::error::AppError::TaskPanic(format!("标记审阅失败: {}", e)))?;
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
            .query_row(
                "SELECT COUNT(DISTINCT source_file) FROM compilations",
                [],
                |row| row.get(0),
            )
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

/// 编译统计摘要
pub struct CompileStats {
    pub total_compilations: i64,
    pub unique_books: i64,
    pub total_cards: i64,
    pub total_tokens: i64,
    pub pending_review: i64,
}
