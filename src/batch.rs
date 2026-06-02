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
            CREATE INDEX IF NOT EXISTS idx_compilations_date ON compilations(compiled_at);

             CREATE TABLE IF NOT EXISTS cards (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                compilation_id INTEGER NOT NULL REFERENCES compilations(id),
                unique_id TEXT NOT NULL,
                title TEXT NOT NULL,
                card_type TEXT NOT NULL,
                quality_score REAL DEFAULT 1.0,
                status TEXT DEFAULT 'Accepted',
                reject_reason TEXT DEFAULT '',
                ref_text TEXT DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_cards_compilation ON cards(compilation_id);
            CREATE INDEX IF NOT EXISTS idx_cards_type ON cards(card_type);

             CREATE TABLE IF NOT EXISTS entities (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                compilation_id INTEGER NOT NULL REFERENCES compilations(id),
                name TEXT NOT NULL,
                entity_type TEXT DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_entities_compilation ON entities(compilation_id);",
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

        Ok(self.db.last_insert_rowid())
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

    /// 批量写入卡片明细
    pub fn record_cards(
        &self,
        compilation_id: i64,
        cards: &[crate::models::Card],
    ) -> Result<usize> {
        let mut count = 0;
        let mut stmt = self
            .db
            .prepare(
                "INSERT INTO cards (compilation_id, unique_id, title, card_type, quality_score, status, reject_reason, ref_text)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .map_err(|e| crate::error::AppError::TaskPanic(format!("准备卡片插入失败: {}", e)))?;

        for card in cards {
            stmt.execute(rusqlite::params![
                compilation_id,
                card.unique_id,
                card.title,
                card.card_type.to_string(),
                card.quality_score,
                format!("{:?}", card.status),
                card.reject_reason,
                card.reference,
            ])
            .ok();
            count += 1;
        }
        Ok(count)
    }

    /// 批量写入实体
    pub fn record_entities(
        &self,
        compilation_id: i64,
        entities: &[crate::models::Entity],
    ) -> Result<usize> {
        let mut count = 0;
        let mut stmt = self
            .db
            .prepare("INSERT INTO entities (compilation_id, name, entity_type) VALUES (?1, ?2, ?3)")
            .map_err(|e| crate::error::AppError::TaskPanic(format!("准备实体插入失败: {}", e)))?;

        for entity in entities {
            stmt.execute(rusqlite::params![
                compilation_id,
                entity.name,
                entity.entity_type
            ])
            .ok();
            count += 1;
        }
        Ok(count)
    }

    /// 获取某次编译的卡片统计
    pub fn card_stats(&self, compilation_id: i64) -> Result<Vec<(String, i64)>> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT card_type, COUNT(*) FROM cards WHERE compilation_id = ?1 GROUP BY card_type ORDER BY COUNT(*) DESC",
            )
            .map_err(|e| crate::error::AppError::TaskPanic(format!("查询卡片统计失败: {}", e)))?;

        let rows = stmt
            .query_map([compilation_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| crate::error::AppError::TaskPanic(format!("查询卡片统计失败: {}", e)))?;

        Ok(rows.filter_map(|r| r.ok()).collect())
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
