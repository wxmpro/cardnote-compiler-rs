//! 批量处理模块 — SQLite 作业队列 + 断点续传
//!
//! CLI 接口:
//!   cardc batch ./pdfs/               批量处理
//!   cardc batch ./pdfs/ --resume      断点续传
//!   cardc batch ./pdfs/ --retry-failed 重试失败
//!   cardc batch-status                查看进度

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::error::Result;

/// 作业状态
#[derive(Debug, Clone, PartialEq)]
pub enum JobStatus {
    Pending,
    Running,
    Done,
    Failed,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Pending => "pending",
            JobStatus::Running => "running",
            JobStatus::Done => "done",
            JobStatus::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "pending" => JobStatus::Pending,
            "running" => JobStatus::Running,
            "done" => JobStatus::Done,
            "failed" => JobStatus::Failed,
            _ => JobStatus::Pending,
        }
    }
}

/// 单个作业记录
#[derive(Debug, Clone)]
pub struct Job {
    pub id: i64,
    pub file_path: String,
    pub status: JobStatus,
    pub output_dir: Option<String>,
    pub error: Option<String>,
    pub card_count: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub retry_count: i64,
}

/// 批量处理报告
#[derive(Debug, Clone, Default)]
pub struct BatchReport {
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub skipped: usize,
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub elapsed: Duration,
}

/// 批量处理运行器
pub struct BatchRunner {
    db: Connection,
}

impl BatchRunner {
    /// 打开或创建 SQLite 批处理数据库
    pub fn new() -> Result<Self> {
        let db_path = Path::new(".cardnote/batch.db");
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let db = Connection::open(db_path).map_err(|e| {
            crate::error::AppError::TaskPanic(format!("无法打开批处理数据库: {}", e))
        })?;

        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS jobs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path TEXT NOT NULL UNIQUE,
                status TEXT NOT NULL DEFAULT 'pending',
                output_dir TEXT,
                error TEXT,
                card_count INTEGER DEFAULT 0,
                prompt_tokens INTEGER DEFAULT 0,
                completion_tokens INTEGER DEFAULT 0,
                retry_count INTEGER DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);",
        )
        .map_err(|e| crate::error::AppError::TaskPanic(format!("无法创建批处理表: {}", e)))?;

        Ok(Self { db })
    }

    /// 扫描目录中的 PDF 文件并入队
    pub fn populate(&self, dir: &Path, recursive: bool) -> Result<usize> {
        let pdfs = crate::scan::find_pdf_files(&dir.to_string_lossy(), recursive)
            .map_err(|e| crate::error::AppError::TaskPanic(format!("扫描失败: {}", e)))?;

        let mut added = 0;
        for pdf in &pdfs {
            let path_str = pdf.to_string_lossy().to_string();
            let result = self.db.execute(
                "INSERT OR IGNORE INTO jobs (file_path, status) VALUES (?1, 'pending')",
                [&path_str],
            );
            if let Ok(changes) = result {
                if changes > 0 {
                    added += 1;
                }
            }
        }
        Ok(added)
    }

    /// 获取指定状态的作业数
    pub fn count_by_status(&self, status: JobStatus) -> Result<usize> {
        let count: i64 = self
            .db
            .query_row(
                "SELECT COUNT(*) FROM jobs WHERE status = ?1",
                [status.as_str()],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(count as usize)
    }

    /// 取一个待处理的作业
    pub fn dequeue(&self) -> Result<Option<Job>> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT id, file_path, status, output_dir, error, card_count,
                        prompt_tokens, completion_tokens, retry_count
                 FROM jobs WHERE status = 'pending'
                 ORDER BY created_at LIMIT 1",
            )
            .map_err(|e| crate::error::AppError::TaskPanic(format!("查询作业失败: {}", e)))?;

        let job = stmt
            .query_row([], |row| {
                Ok(Job {
                    id: row.get(0)?,
                    file_path: row.get(1)?,
                    status: JobStatus::from_str(&row.get::<_, String>(2)?),
                    output_dir: row.get(3)?,
                    error: row.get(4)?,
                    card_count: row.get(5)?,
                    prompt_tokens: row.get(6)?,
                    completion_tokens: row.get(7)?,
                    retry_count: row.get(8)?,
                })
            })
            .ok();
        Ok(job)
    }

    /// 标记作业为运行中
    pub fn mark_running(&self, id: i64) -> Result<()> {
        self.db
            .execute("UPDATE jobs SET status = 'running' WHERE id = ?1", [id])
            .map_err(|e| crate::error::AppError::TaskPanic(format!("更新作业状态失败: {}", e)))?;
        Ok(())
    }

    /// 标记作业为完成
    pub fn mark_done(&self, id: i64, output_dir: &str, card_count: usize) -> Result<()> {
        self.db
            .execute(
                "UPDATE jobs SET status = 'done', output_dir = ?1, card_count = ?2 WHERE id = ?3",
                rusqlite::params![output_dir, card_count as i64, id],
            )
            .map_err(|e| crate::error::AppError::TaskPanic(format!("更新作业状态失败: {}", e)))?;
        Ok(())
    }

    /// 标记作业为失败（自动过滤 API key 等敏感信息）
    pub fn mark_failed(&self, id: i64, error: &str) -> Result<()> {
        let safe_error = Self::sanitize_error(error);
        self.db
            .execute(
                "UPDATE jobs SET status = 'failed', error = ?1, retry_count = retry_count + 1 WHERE id = ?2",
                rusqlite::params![safe_error, id],
            )
            .map_err(|e| crate::error::AppError::TaskPanic(format!("更新作业状态失败: {}", e)))?;
        Ok(())
    }

    /// 过滤错误消息中的敏感信息（API key、token 等）
    fn sanitize_error(error: &str) -> String {
        let mut s = error.to_string();
        // 过滤常见 API key 前缀模式: sk-, sk-or-, anthropic-, etc.
        let patterns = [
            ("sk-or-", "sk-***-"),
            ("sk-", "sk-***"),
            ("Bearer ", "Bearer ***"),
        ];
        for (pattern, replacement) in &patterns {
            if let Some(pos) = s.find(pattern) {
                let start = pos + pattern.len();
                // 替换 key 部分为 *** (保留前缀供调试)
                let end = s[start..]
                    .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                    .map(|i| start + i)
                    .unwrap_or(s.len());
                s.replace_range(start..end, "***");
            }
        }
        s
    }

    /// 将失败的作业重置为待处理（用于 --retry-failed）
    pub fn reset_failed(&self) -> Result<usize> {
        let count = self
            .db
            .execute(
                "UPDATE jobs SET status = 'pending' WHERE status = 'failed'",
                [],
            )
            .map_err(|e| crate::error::AppError::TaskPanic(format!("重置失败作业出错: {}", e)))?;
        Ok(count)
    }

    /// 获取批处理统计信息
    pub fn status(&self) -> Result<String> {
        let total: i64 = self
            .db
            .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))
            .unwrap_or(0);
        let pending = self.count_by_status(JobStatus::Pending)?;
        let running = self.count_by_status(JobStatus::Running)?;
        let done = self.count_by_status(JobStatus::Done)?;
        let failed = self.count_by_status(JobStatus::Failed)?;

        let mut lines = vec![
            "╔══════════════════════════════════════════════╗".to_string(),
            "║           CardNote Batch 状态                ║".to_string(),
            "╠══════════════════════════════════════════════╣".to_string(),
            format!(
                "║  Total: {:>4} | Done: {:>4} | Failed: {:>3} | Pending: {:>3} ║",
                total, done, failed, pending
            ),
            "╚══════════════════════════════════════════════╝".to_string(),
        ];

        if running > 0 {
            lines.push(format!("  Running: {}", running));
        }

        Ok(lines.join("\n"))
    }

    /// 列出失败作业的详情
    pub fn failed_details(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT file_path, error, retry_count FROM jobs WHERE status = 'failed' ORDER BY id",
            )
            .map_err(|e| crate::error::AppError::TaskPanic(format!("查询失败作业出错: {}", e)))?;

        let mut details = Vec::new();
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| crate::error::AppError::TaskPanic(format!("查询失败作业出错: {}", e)))?;

        for row in rows {
            if let Ok((path, error, retries)) = row {
                details.push(format!("  {} — {} (重试 {} 次)", path, error, retries));
            }
        }
        Ok(details)
    }
}
