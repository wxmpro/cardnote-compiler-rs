use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::api::LlmClient;
use crate::doc_type::{DocTypeDetector, DocumentType};
use crate::error::{AppError, Result};
use crate::models::{
    Card, ChunkInfo, CompilationDiagnostics, CompilationResult, Entity, KnowledgeGraph, LlmMessage,
    StageFail, Summary,
};
use crate::stages::common::{ChatFn, ChatJsonFn};
use crate::stages::entities::unify_entities;

// ═══════════════════════════════════════════════════════
//  共享 FNV-1a 哈希函数
// ═══════════════════════════════════════════════════════

/// 稳定的 FNV-1a 字符串哈希
/// [M5] 使用稳定的 FNV-1a 哈希，避免 Rust 版本升级后 DefaultHasher 变化导致缓存全部失效
pub fn fnv1a_hash_str(data: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in data.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:x}", hash)
}

/// 分块编译结果（内部使用）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ChunkResult {
    document: String,
    title_path: String,
    entities: Vec<Entity>,
    cards: Vec<Card>,
}

/// 编译缓存（用于断点续编译）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompileCache {
    /// 输入文档的哈希（用于验证缓存是否对应同一文档）
    document_hash: String,
    /// 每个块的编译结果
    chunk_results: Vec<Option<ChunkResult>>,
    /// 缓存版本号
    version: u32,
}

impl CompileCache {
    const CURRENT_VERSION: u32 = 2; // v2: key 包含 provider+model
    const CACHE_DIR: &'static str = ".cardc_cache";

    /// 计算缓存 key（包含 provider + model，避免跨 provider 脏缓存）
    fn cache_key(provider: &str, model: &str, document: &str) -> String {
        let combined = format!(
            "v{}|{}|{}|{}",
            Self::CURRENT_VERSION,
            provider,
            model,
            fnv1a_hash_str(document)
        );
        fnv1a_hash_str(&combined)
    }

    /// 获取缓存文件路径（基于 source_file + provider + model + doc_hash）
    fn cache_path(source_file: &str, provider: &str, model: &str, document: &str) -> PathBuf {
        let cache_dir = Path::new(Self::CACHE_DIR);
        std::fs::create_dir_all(cache_dir).ok();
        let safe_source = source_file.replace(['/', '\\', ':'], "_");
        let provider_model_hash = fnv1a_hash_str(&format!("{}:{}", provider, model));
        let doc_hash = fnv1a_hash_str(document);
        let filename = format!(
            "{}_{}_{}.cache.json",
            safe_source,
            &provider_model_hash[..8],
            &doc_hash[..8]
        );
        cache_dir.join(filename)
    }

    /// 加载缓存
    fn load(source_file: &str, provider: &str, model: &str, document: &str) -> Option<Self> {
        let path = Self::cache_path(source_file, provider, model, document);
        if !path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(&path).ok()?;
        let cache: CompileCache = serde_json::from_str(&content).ok()?;

        // 验证版本和缓存 key（包含 provider+model）
        let expected_key = Self::cache_key(provider, model, document);
        if cache.version != Self::CURRENT_VERSION || cache.document_hash != expected_key {
            return None;
        }

        Some(cache)
    }

    /// 保存缓存
    fn save(&self, source_file: &str, provider: &str, model: &str, document: &str) -> Result<()> {
        let path = Self::cache_path(source_file, provider, model, document);
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| AppError::TaskPanic(format!("缓存序列化失败: {}", e)))?;
        std::fs::write(&path, content)
            .map_err(|e| AppError::TaskPanic(format!("缓存写入失败: {}", e)))?;
        Ok(())
    }

    /// 创建新缓存
    fn new(provider: &str, model: &str, document: &str, chunk_count: usize) -> Self {
        Self {
            document_hash: Self::cache_key(provider, model, document),
            chunk_results: vec![None; chunk_count],
            version: Self::CURRENT_VERSION,
        }
    }

    /// 更新块结果
    fn set_chunk_result(&mut self, idx: usize, result: ChunkResult) {
        if idx < self.chunk_results.len() {
            self.chunk_results[idx] = Some(result);
        }
    }

    /// 获取已完成的块索引
    fn completed_indices(&self) -> Vec<usize> {
        self.chunk_results
            .iter()
            .enumerate()
            .filter_map(|(i, r)| r.is_some().then_some(i))
            .collect()
    }

    /// 获取未完成的块索引
    fn pending_indices(&self) -> Vec<usize> {
        self.chunk_results
            .iter()
            .enumerate()
            .filter_map(|(i, r)| r.is_none().then_some(i))
            .collect()
    }
}

/// 清理过期缓存文件
/// 策略：删除超过 30 天未修改的 .json/.cache.json 文件；若总文件数超过 200，额外删除最旧的
fn cleanup_cache_dir() {
    const MAX_AGE_DAYS: u64 = 30;
    const MAX_FILES: usize = 200;
    let cutoff =
        std::time::SystemTime::now() - std::time::Duration::from_secs(MAX_AGE_DAYS * 86400);

    let dirs = [".cardc_cache"];
    for dir in &dirs {
        let path = Path::new(dir);
        if !path.exists() {
            continue;
        }

        let mut files: Vec<(std::fs::DirEntry, std::time::SystemTime)> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let file_name = entry.file_name();
                let name = file_name.to_string_lossy();
                if !name.ends_with(".json") && !name.ends_with(".cache.json") {
                    continue;
                }
                if let Ok(meta) = entry.metadata()
                    && meta.is_file()
                {
                    let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    if mtime < cutoff {
                        if let Err(e) = std::fs::remove_file(entry.path()) {
                            eprintln!("  ⚠ 缓存清理失败: {} — {}", entry.path().display(), e);
                        }
                    } else {
                        files.push((entry, mtime));
                    }
                }
            }
        }

        // 若剩余文件数仍超过上限，按修改时间排序删除最旧的
        if files.len() > MAX_FILES {
            files.sort_by_key(|a| a.1);
            let to_remove = files.len() - MAX_FILES;
            for (entry, _) in files.into_iter().take(to_remove) {
                if let Err(e) = std::fs::remove_file(entry.path()) {
                    eprintln!("  ⚠ 缓存清理失败: {} — {}", entry.path().display(), e);
                }
            }
        }
    }
}

/// 编译上下文（可 Clone，用于并行任务）
#[derive(Clone, Debug)]
struct CompileContext {
    client: Arc<LlmClient>,
    prompts: Arc<HashMap<String, String>>,
    #[allow(dead_code)]
    stage_models: Arc<HashMap<String, String>>,
    /// 根据模型上下文动态计算的单块最大字符数
    chunk_size: usize,
    /// 输出目录（用于实时写入每块结果）
    output_dir: Option<std::path::PathBuf>,
}

impl CompileContext {
    async fn call_llm(&self, messages: Vec<LlmMessage>, max_tokens: Option<u32>) -> Result<String> {
        let response = self.client.chat(messages, max_tokens).await?;
        self.client.extract_content(&response)
    }

    async fn call_llm_json(
        &self,
        messages: Vec<LlmMessage>,
        max_tokens: Option<u32>,
    ) -> Result<serde_json::Value> {
        self.client.chat_json(messages, max_tokens).await
    }

    /// 调用 JSON mode 并返回（解析后 JSON, 原始文本）
    async fn call_llm_json_with_raw(
        &self,
        messages: Vec<LlmMessage>,
        max_tokens: Option<u32>,
    ) -> Result<(serde_json::Value, String)> {
        // 走 JSON mode（response_format: json_object），API 返回纯 JSON
        let json = self.call_llm_json(messages.clone(), max_tokens).await?;
        let raw = serde_json::to_string_pretty(&json).unwrap_or_default();
        Ok((json, raw))
    }

    fn load_prompt(&self, name: &str) -> Result<String> {
        self.prompts
            .get(name)
            .cloned()
            .ok_or_else(|| AppError::PromptLoad(format!("Prompt 模板文件不存在: {}.md", name)))
    }

    /// 获取指定阶段的专用 client（支持 Tiered 模型）
    #[allow(dead_code)]
    fn client_for_stage(&self, stage: &str) -> LlmClient {
        if let Some(model) = self.stage_models.get(stage) {
            self.client.with_model(model)
        } else {
            (*self.client).clone()
        }
    }

}

impl ChatFn for CompileContext {
    async fn call_chat(
        &self,
        messages: Vec<LlmMessage>,
        max_tokens: Option<u32>,
    ) -> Result<String> {
        self.call_llm(messages, max_tokens).await
    }
}

impl ChatJsonFn for CompileContext {
    async fn call_json(
        &self,
        messages: Vec<LlmMessage>,
        max_tokens: Option<u32>,
    ) -> Result<serde_json::Value> {
        self.call_llm_json(messages, max_tokens).await
    }
}

/// 知识编译流水线
#[derive(Debug, Clone)]
pub struct Pipeline {
    ctx: CompileContext,
}

impl Pipeline {
    pub fn new(client: LlmClient, output_dir: Option<std::path::PathBuf>) -> Self {
        let mut prompts = HashMap::new();

        // 尝试多个候选路径查找 prompts 目录
        let mut candidates: Vec<PathBuf> = Vec::new();

        // 1. 环境变量优先（支持项目移动或二进制单独分发）
        if let Ok(env_path) = std::env::var("CARDNOTE_PROMPTS_DIR") {
            candidates.push(PathBuf::from(env_path));
        }

        // 2. 从 exe 位置推断
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()));
        if let Some(ref dir) = exe_dir {
            candidates.push(dir.join("../prompts")); // exe 旁（如 bin/）
            candidates.push(dir.join("../../prompts")); // target/release/ 等
            candidates.push(dir.join("../../../prompts")); // 更深一层
        }

        // 3. 运行时工作目录
        candidates.push(Path::new("prompts").to_path_buf());

        for dir in &candidates {
            prompts = load_prompts_from_dir(dir);
            if !prompts.is_empty() {
                break;
            }
        }

        // 加载阶段模型配置（Tiered Strategy，默认全部使用同一模型）
        let stage_models = crate::config::StageModelConfig::from_env();
        let mut stage_model_map = HashMap::new();
        for (stage, model_opt) in [
            ("summary", stage_models.summary),
            ("entities", stage_models.entities),
            ("cards", stage_models.cards),
            ("graph", stage_models.graph),
        ] {
            if let Some(model) = model_opt {
                stage_model_map.insert(stage.to_string(), model);
            }
        }

        // 异步启动缓存清理，不阻塞 Pipeline 初始化
        tokio::spawn(async move {
            cleanup_cache_dir();
        });

        // 根据模型上下文长度与最大输出 tokens 动态计算分块大小
        let ctx_len = client.context_length().unwrap_or(200_000);
        let max_out = client.max_output_tokens().unwrap_or(8192);
        let chunk_size = crate::config::chunk_size_for_context(ctx_len, max_out);
        println!("  模型上下文: {} tokens | 最大输出: {} tokens → 单块上限: {} 字符", ctx_len, max_out, chunk_size);

        Self {
            ctx: CompileContext {
                client: Arc::new(client),
                prompts: Arc::new(prompts),
                stage_models: Arc::new(stage_model_map),
                chunk_size,
                output_dir,
            },
        }
    }

    /// 运行完整编译流程
    pub async fn run(
        &self,
        document: &str,
        source_file: &str,
        book_title: &str,
    ) -> Result<CompilationResult> {
        println!("{}", "=".repeat(60));
        println!("知识编译引擎启动 — 《{}》", book_title);
        println!("{}", "=".repeat(60));
        // [H3] 使用 chars().count() 获取真实字符数（中文不再虚高 3 倍）
        let doc_char_count = document.chars().count();
        println!("  文档大小: {} 字符", doc_char_count);
        let doc_detection = DocTypeDetector::detect_with_confidence(document);

        // [H3] 按字符数判断是否需要分块
        if document.chars().count() <= self.ctx.chunk_size {
            self.run_single(document, source_file, doc_detection.doc_type, book_title)
                .await
        } else {
            self.run_map_reduce(document, source_file, doc_detection.doc_type, book_title)
                .await
        }
    }

    /// 单文档 Unified 编译
    async fn run_single(
        &self,
        document: &str,
        source_file: &str,
        doc_type: DocumentType,
        book_title: &str,
    ) -> Result<CompilationResult> {
        println!("\n[模式] 单轮 Unified 编译（1 次请求）");
        println!("{}", "-".repeat(60));

        let mut diagnostics = CompilationDiagnostics::default();
        let ctx = self.ctx.clone();

        let doc_char_count = document.chars().count();

        // 第一次：核心类型
        let core_types = "术语卡、新知卡、反常识卡、行动卡";
        let (core_result, _core_raw) = match with_retry("unified_core", || {
            compile_chunk_unified(
                ctx.clone(),
                document.to_string(),
                String::new(),
                doc_type,
                book_title.to_string(),
                doc_char_count,
                1,
                core_types,
            )
        })
        .await
        {
            Ok((r, raw)) => (r, raw),
            Err(e) => {
                let err_msg = format!("{}", e);
                println!("  ✗ 核心类型编译失败: {}", err_msg);
                diagnostics.failures.push(StageFail {
                    stage: "unified_core".to_string(),
                    error: err_msg,
                    retry_count: 3,
                    final_status: "failed".to_string(),
                });
                (ChunkResult::default(), String::new())
            }
        };

        // 第二次：补充类型
        let supp_types = "人物卡、金句卡、综述卡";
        let (supp_result, _supp_raw) = match with_retry("unified_supp", || {
            compile_chunk_unified(
                ctx.clone(),
                document.to_string(),
                String::new(),
                doc_type,
                book_title.to_string(),
                doc_char_count,
                1,
                supp_types,
            )
        })
        .await
        {
            Ok((r, raw)) => (r, raw),
            Err(e) => {
                let err_msg = format!("{}", e);
                println!("  ✗ 补充类型编译失败: {}", err_msg);
                diagnostics.failures.push(StageFail {
                    stage: "unified_supp".to_string(),
                    error: err_msg,
                    retry_count: 3,
                    final_status: "failed".to_string(),
                });
                (ChunkResult::default(), String::new())
            }
        };

        // 合并两次结果
        let mut all_entities = core_result.entities;
        all_entities.extend(supp_result.entities);
        let mut all_cards = core_result.cards;
        all_cards.extend(supp_result.cards);

        // 为单块结果分配 UUID 和 chunk_id
        assign_unique_ids_with_base(&mut all_cards, 0);
        for card in &mut all_cards {
            card.chunk_id = "chunk_00".to_string();
        }

        println!("\n{}", "=".repeat(60));
        println!("编译完成！");
        println!(
            "  实体: {} 个  |  卡片: {} 张",
            all_entities.len(),
            all_cards.len()
        );
        if !diagnostics.failures.is_empty() {
            println!(
                "  ⚠ {} 个阶段失败，详见诊断报告",
                diagnostics.failures.len()
            );
        }
        println!("{}", "=".repeat(60));

        // 输出 LLM 用量报告并清理内存
        let report = self.ctx.client.usage_report().await;
        println!("\n{}", report);
        self.ctx.client.clear_usage().await;

        Ok(CompilationResult {
            source_file: source_file.to_string(),
            summary: Summary::default(),
            cards: all_cards,
            graph: KnowledgeGraph {
                entities: all_entities,
                relations: Vec::new(),
            },
            chunks: Vec::new(),
            diagnostics,
        })
    }

    // ── Map-Reduce 编译 ──

    async fn run_map_reduce(
        &self,
        document: &str,
        source_file: &str,
        doc_type: DocumentType,
        book_title: &str,
    ) -> Result<CompilationResult> {
        println!("\n[模式] 分块 Map-Reduce 编译（阈值: {} 字符）", self.ctx.chunk_size);
        println!("{}", "-".repeat(60));

        // 1. Split
        println!("\n[1/4] 语义分块...");
        let chunks = self.semantic_chunk(document);
        if chunks.is_empty() {
            return Err(AppError::TaskPanic(
                "文档分块后无有效内容，可能是空文档或格式无法解析".to_string(),
            ));
        }
        let chunks_len = chunks.len();
        let total_doc_chars = document.chars().count();
        println!("  分成 {} 个语义块", chunks_len);

        // 2. Map — 串行编译（支持断点续编译）
        println!("\n[2/4] Map 阶段 — 串行编译...");
        let mut chunk_results: Vec<ChunkResult> = vec![ChunkResult::default(); chunks_len];
        let mut failed_chunks: Vec<(usize, String)> = Vec::new();

        let provider = self.ctx.client.provider_id().to_string();
        let model = self.ctx.client.model.clone();

        // 2a. 检查编译缓存（断点续编译，key 含 provider+model）
        let mut cache = CompileCache::load(source_file, &provider, &model, document)
            .unwrap_or_else(|| CompileCache::new(&provider, &model, document, chunks_len));
        let completed = cache.completed_indices();
        if !completed.is_empty() {
            println!("  💾 发现编译缓存，已完成的块: {:?}", completed);
            for idx in &completed {
                if let Some(result) = cache.chunk_results[*idx].clone() {
                    chunk_results[*idx] = result;
                }
            }
        }
        let pending = cache.pending_indices();
        if !pending.is_empty() {
            println!("  🔄 继续编译未完成的块: {:?}", pending);
        }

        // 2b. 编译未完成的块（两次触发：核心类型 + 补充类型）
        const CORE_TYPES: &str = "术语卡、新知卡、反常识卡、行动卡";
        const SUPP_TYPES: &str = "人物卡、金句卡、综述卡";
        let mut last_ts: i64 = 0;
        for idx in &pending {
            let idx = *idx;
            let ctx = self.ctx.clone();
            let (title, doc) = chunks[idx].clone();
            println!("  处理块 {}/{}...", idx + 1, chunks_len);
            let bt = book_title.to_string();

            // 第一次：核心类型
            let core_result = match compile_chunk_unified(
                ctx.clone(),
                doc.clone(),
                title.clone(),
                doc_type,
                bt.clone(),
                total_doc_chars,
                chunks_len,
                CORE_TYPES,
            )
            .await
            {
                Ok((r, raw)) => {
                    println!(
                        "    ✓ 核心类型: {} 实体 | {} 卡片",
                        r.entities.len(),
                        r.cards.len()
                    );
                    Some((r, raw))
                }
                Err(e) => {
                    println!("    ✗ 核心类型编译失败: {}", e);
                    None
                }
            };

            // 第二次：补充类型
            let supp_result = match compile_chunk_unified(
                ctx.clone(),
                doc.clone(),
                title.clone(),
                doc_type,
                bt,
                total_doc_chars,
                chunks_len,
                SUPP_TYPES,
            )
            .await
            {
                Ok((r, raw)) => {
                    println!(
                        "    ✓ 补充类型: {} 实体 | {} 卡片",
                        r.entities.len(),
                        r.cards.len()
                    );
                    Some((r, raw))
                }
                Err(e) => {
                    println!("    ✗ 补充类型编译失败: {}", e);
                    None
                }
            };

            // 合并两次结果
            let mut merged = ChunkResult {
                document: doc,
                title_path: title,
                ..Default::default()
            };
            let mut all_raw = String::new();
            if let Some((r, raw)) = core_result {
                merged.entities.extend(r.entities);
                merged.cards.extend(r.cards);
                all_raw.push_str(&raw);
            }
            if let Some((r, raw)) = supp_result {
                merged.entities.extend(r.entities);
                merged.cards.extend(r.cards);
                if !all_raw.is_empty() {
                    all_raw.push_str("\n\n---\n\n");
                }
                all_raw.push_str(&raw);
            }

            if merged.cards.is_empty() {
                let err_msg = format!("块 {} 两次编译均无输出", idx + 1);
                println!("  ✗ {}", err_msg);
                failed_chunks.push((idx, err_msg));
                continue;
            }

            // 分配跨块唯一的 UUID 和 chunk_id
            last_ts = assign_unique_ids_with_base(&mut merged.cards, last_ts);
            for card in &mut merged.cards {
                card.chunk_id = format!("chunk_{:02}", idx);
            }

            println!(
                "  ✓ 块 {}/{} 合并完成 — 实体 {} 个 | 卡片 {} 张",
                idx + 1,
                chunks_len,
                merged.entities.len(),
                merged.cards.len()
            );

            // 实时写入 output 目录
            if let Some(ref out_dir) = ctx.output_dir {
                let chunks_dir = out_dir.join("chunks");
                std::fs::create_dir_all(&chunks_dir).ok();
                let prefix = format!("chunk_{:02}", idx);
                std::fs::write(chunks_dir.join(format!("{}_raw.json", prefix)), &all_raw).ok();
                if let Ok(json) = serde_json::to_string_pretty(&merged) {
                    std::fs::write(chunks_dir.join(format!("{}.json", prefix)), json).ok();
                }
                let cards_md: Vec<String> = merged.cards.iter().map(|c| c.to_markdown()).collect();
                std::fs::write(
                    chunks_dir.join(format!("{}_cards.md", prefix)),
                    cards_md.join("\n\n---\n\n"),
                )
                .ok();
                let mut entities_md = String::from("# 实体列表\n\n");
                for e in &merged.entities {
                    entities_md.push_str(&format!("- **{}** ({}): {}\n", e.name, e.entity_type, e.context));
                }
                std::fs::write(chunks_dir.join(format!("{}_entities.md", prefix)), entities_md).ok();
            }
            chunk_results[idx] = merged.clone();
            cache.set_chunk_result(idx, merged);
            cache.save(source_file, &provider, &model, document).ok();
        }

        // 2c. 对失败块进行整体重试（一次）
        if !failed_chunks.is_empty() {
            let fail_rate = failed_chunks.len() as f32 / chunks_len as f32;
            if fail_rate > 0.2 {
                return Err(AppError::TaskPanic(format!(
                    "分块编译失败率 {:.0}% ({} / {})，已中断。错误: {}",
                    fail_rate * 100.0,
                    failed_chunks.len(),
                    chunks_len,
                    failed_chunks
                        .iter()
                        .map(|(_, e)| e.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                )));
            }
            println!(
                "\n  ⚠ {} 个块编译失败 ({}%)，尝试整体重试...",
                failed_chunks.len(),
                (fail_rate * 100.0) as u32
            );

            // 需要重新分块获取失败的块的标题和内容
            let chunks_for_retry = self.semantic_chunk(document);
            let mut still_failed: Vec<(usize, String)> = Vec::new();

            for (idx, _) in &failed_chunks {
                if let Some((title, doc)) = chunks_for_retry.get(*idx) {
                    println!("  重试块 {}/{}...", idx + 1, chunks_len);
                    let ctx = self.ctx.clone();
                    let bt = book_title.to_string();

                    // 重试：核心类型
                    let core_retry = match compile_chunk_unified(
                        ctx.clone(),
                        doc.clone(),
                        title.clone(),
                        doc_type,
                        bt.clone(),
                        total_doc_chars,
                        chunks_len,
                        CORE_TYPES,
                    )
                    .await
                    {
                        Ok((r, _)) => Some(r),
                        Err(e) => {
                            println!("    ✗ 核心类型重试失败: {}", e);
                            None
                        }
                    };

                    // 重试：补充类型
                    let supp_retry = match compile_chunk_unified(
                        ctx.clone(),
                        doc.clone(),
                        title.clone(),
                        doc_type,
                        bt,
                        total_doc_chars,
                        chunks_len,
                        SUPP_TYPES,
                    )
                    .await
                    {
                        Ok((r, _)) => Some(r),
                        Err(e) => {
                            println!("    ✗ 补充类型重试失败: {}", e);
                            None
                        }
                    };

                    // 合并重试结果
                    let mut merged = ChunkResult {
                        document: doc.clone(),
                        title_path: title.clone(),
                        ..Default::default()
                    };
                    if let Some(r) = core_retry {
                        merged.entities.extend(r.entities);
                        merged.cards.extend(r.cards);
                    }
                    if let Some(r) = supp_retry {
                        merged.entities.extend(r.entities);
                        merged.cards.extend(r.cards);
                    }

                    if !merged.cards.is_empty() {
                        last_ts = assign_unique_ids_with_base(&mut merged.cards, last_ts);
                        for card in &mut merged.cards {
                            card.chunk_id = format!("chunk_{:02}", idx);
                        }
                        println!(
                            "  ✓ 块 {}/{} 重试成功 — 实体 {} 个 | 卡片 {} 张",
                            idx + 1,
                            chunks_len,
                            merged.entities.len(),
                            merged.cards.len()
                        );
                        if let Some(ref out_dir) = ctx.output_dir {
                            let chunks_dir = out_dir.join("chunks");
                            std::fs::create_dir_all(&chunks_dir).ok();
                            let prefix = format!("chunk_{:02}", idx);
                            if let Ok(json) = serde_json::to_string_pretty(&merged) {
                                std::fs::write(chunks_dir.join(format!("{}.json", prefix)), json).ok();
                            }
                        }
                        chunk_results[*idx] = merged;
                    } else {
                        let err_msg = format!("块 {} 两次重试均无输出", idx + 1);
                        println!("  ✗ {}", err_msg);
                        still_failed.push((*idx, err_msg));
                    }
                }
            }

            failed_chunks = still_failed;

            if !failed_chunks.is_empty() {
                println!(
                    "\n  ⚠ {} 个块重试后仍然失败，继续处理成功的 {} 个块",
                    failed_chunks.len(),
                    chunks_len - failed_chunks.len()
                );
            } else {
                println!("\n  ✓ 所有块重试成功");
            }
        }

        // 3. Reduce
        println!("\n[3/3] Reduce 阶段 — 合并去重...");

        // 先生成 ChunkInfo（在 move 数据之前）
        let chunk_infos: Vec<ChunkInfo> = chunk_results
            .iter()
            .map(|c| ChunkInfo {
                title_path: c.title_path.clone(),
                size: c.document.chars().count(),
                entities: c.entities.len(),
                cards: c.cards.len(),
                relations: 0,
            })
            .collect();

        // 质量过滤：使用 chunk 级别源文本而非完整 document（性能提升 10x+）
        for r in &mut chunk_results {
            let (chunk_filtered, _stats) = crate::quality::filter_cards_with_source(
                &r.cards,
                &r.document,
                &crate::quality::CardLintConfig::default(),
            );
            r.cards = chunk_filtered;
        }

        let mut all_entities = Vec::new();
        let mut all_cards = Vec::new();

        for r in &mut chunk_results {
            all_entities.append(&mut r.entities);
            all_cards.append(&mut r.cards);
        }

        let (unique_entities, entity_stats, _entity_name_map) = unify_entities(&all_entities);
        if entity_stats.merged_groups > 0 {
            eprintln!(
                "  ✓ 实体统一: {} 个实体 → {} 个 (合并 {} 组, 消除 {} 个重复)",
                entity_stats.original_count,
                entity_stats.unified_count,
                entity_stats.merged_groups,
                entity_stats.eliminated_duplicates
            );
        }
        let unique_cards = dedup_cards(&all_cards);

        println!(
            "  合并后: 实体 {} 个 | 卡片 {} 张",
            unique_entities.len(),
            unique_cards.len()
        );

        println!("\n{}", "=".repeat(60));
        println!("分块编译完成！");
        println!("  分块数: {}", chunk_results.len());
        println!(
            "  实体: {} 个  |  卡片: {} 张",
            unique_entities.len(),
            unique_cards.len()
        );
        println!("{}", "=".repeat(60));

        // 输出 LLM 用量报告并清理内存
        let report = self.ctx.client.usage_report().await;
        println!("\n{}", report);
        self.ctx.client.clear_usage().await;

        Ok(CompilationResult {
            source_file: source_file.to_string(),
            summary: Summary::default(),
            cards: unique_cards,
            graph: KnowledgeGraph {
                entities: unique_entities,
                relations: Vec::new(),
            },
            chunks: chunk_infos,
            diagnostics: CompilationDiagnostics {
                failures: failed_chunks
                    .iter()
                    .map(|(idx, err_msg)| StageFail {
                        stage: format!("chunk_{}", idx),
                        error: err_msg.clone(),
                        retry_count: 1,
                        final_status: "failed".to_string(),
                    })
                    .collect(),
                degradations: Vec::new(),
                retries: Vec::new(),
            },
        })
    }

    // ── 语义分块 ──

    fn semantic_chunk(&self, document: &str) -> Vec<(String, String)> {
        let lines: Vec<&str> = document.split('\n').collect();
        let mut chunks: Vec<(String, String)> = Vec::new();
        let mut current_doc = String::new();
        let mut title_stack: Vec<String> = Vec::new();
        let mut current_size = 0;

        let heading_re = heading_regex();

        for line in &lines {
            if let Some(caps) = heading_re.captures(line) {
                let level = caps[1].len();
                let title = caps[2].trim().to_string();

                if current_size >= self.ctx.chunk_size - 5000 {
                    let overlap = extract_overlap(&current_doc, &title_stack);
                    flush_chunk(&mut chunks, &current_doc, &title_stack);
                    current_doc = overlap;
                    current_size = current_doc.len();
                }

                while title_stack.len() >= level {
                    title_stack.pop();
                }
                title_stack.push(title);

                current_doc.push_str(line);
                current_doc.push('\n');
                current_size += line.len() + 1;
                continue;
            }

            current_doc.push_str(line);
            current_doc.push('\n');
            current_size += line.len() + 1;

            if current_size >= self.ctx.chunk_size {
                let overlap = extract_overlap(&current_doc, &title_stack);
                flush_chunk(&mut chunks, &current_doc, &title_stack);
                current_doc = overlap;
                current_size = current_doc.len();
            }
        }

        if !current_doc.trim().is_empty() {
            flush_chunk(&mut chunks, &current_doc, &title_stack);
        }

        chunks.retain(|(_, doc)| doc.trim().len() > 200);

        if chunks.is_empty() && !document.trim().is_empty() {
            chunks.push(("".to_string(), document.trim().to_string()));
        }

        chunks
    }

}

/// 辅助函数：带重试的执行（3次重试 + 指数退避）
async fn with_retry<T, F, Fut>(name: &str, f: F) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let max_retries = 3;
    let mut last_err = None;
    let mut attempt = 0;
    while attempt < max_retries {
        attempt += 1;
        match f().await {
            Ok(result) => return Ok(result),
            Err(crate::error::AppError::RateLimited(seconds)) => {
                // 429: 使用 API 指定的等待时间，不计入重试次数
                eprintln!(
                    "    ⏳ {} 触发速率限制 (429)，等待 {}s...",
                    name, seconds
                );
                tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
                attempt = 0; // 重置计数，429 不算失败
                continue;
            }
            Err(e) => {
                let msg = format!("{}: {}", name, e);
                eprintln!(
                    "    ⚠ {} 失败 (尝试 {}/{}): {}",
                    name, attempt, max_retries, msg
                );
                last_err = Some(e);
                if attempt < max_retries {
                    let delay = std::time::Duration::from_secs(2u64.pow(attempt as u32));
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
    Err(last_err
        .unwrap_or_else(|| crate::error::AppError::TaskPanic(format!("{} 全部重试失败", name))))
}

/// Unified 编译模式：一次 LLM 请求生成 entities + cards
///
/// 文档类型范围：认知科学、心理学、计算机科学、教材类知识文档。
/// 不涉及散文、小说、诗歌等文学创作。
async fn compile_chunk_unified(
    ctx: CompileContext,
    document: String,
    title_path: String,
    _doc_type: DocumentType,
    _book_title: String,
    total_doc_chars: usize,
    total_chunks: usize,
    allowed_types: &str,
) -> Result<(ChunkResult, String)> {
    let ctx_ref = &ctx;

    // 加载 unified prompt
    let prompt_template = ctx_ref.load_prompt("unified")?;

    // ═══════════════════════════════════════════════════════════════════
    //  卡片数量计算理论推导（由字符数决定，不是拍脑袋）
    // ═══════════════════════════════════════════════════════════════════
    //
    //  1. 每张卡片需要多少原始素材？
    //     从 prompt 卡片质量标准反推：
    //     - 术语卡：定义+解释+1-3个例子 ≈ 300字输出 → 需1500字素材
    //     - 新知卡：已知+新知+例子       ≈ 400字输出 → 需2500字素材
    //     - 人物卡：定位+小传+贡献+时间线 ≈ 500字输出 → 需3000字素材
    //     - 行动卡：原理+步骤+检验       ≈ 400字输出 → 需2000字素材
    //     - 金句卡：原文+解读+仿写       ≈ 300字输出 → 需500字素材
    //     平均：一张卡片 ≈ 2500 字符原始素材
    //
    //  2. 信息密度系数（可提取字符 / 总字符）
    //     - 学术专著：0.5
    //     - 知识类书籍：0.45
    //     - 通俗读物：0.35
    //     - 散文/小说：0.2
    //
    //  3. 全书目标卡片数公式
    //     目标 = 总字符数 × 信息密度系数 / 每张卡片所需素材
    //          = 总字符数 × 0.45 / 2500
    //          = 总字符数 / 5556
    //
    //  4. 单块卡片数
    //     不设硬上限。只有文档 > chunk_size 时才分块。
    //     分块时：每块目标 = ceil(全书目标 / 块数)
    //
    //  5. 分块判断
    //     chunk_size = min(input_based, output_based).clamp(10K, 500K)
    //     input_based  = (context_length - 2800) / 1.3
    //     output_based = (max_output_tokens / 700) × 4000
    //     文档 > chunk_size → 分块
    //     文档 ≤ chunk_size → 1块1次请求
    //
    //  示例：《人生模式》436,320 字符
    //     目标 = 436,320 / 5556 ≈ 78 张
    //     chunk_size (1M context / 150K output) = 500K
    //     436K < 500K → 1块 → 1次请求 → 一次输出78张
    // ═══════════════════════════════════════════════════════════════════

    let total_chunks = total_chunks.max(1);

    // 防御性检查：空文档直接返回空结果，避免后续 .max(8) 变成要求生成8张空卡片
    if total_doc_chars == 0 {
        return Ok((ChunkResult { cards: vec![], ..Default::default() }, "空文档，跳过处理".to_string()));
    }

    // 全书目标卡片数 = 总字符数 / 5556
    // 5556 = 2500(素材/张) / 0.45(密度系数)，取整便于计算
    let total_target = (total_doc_chars as f64 / 5556.0).ceil() as usize;
    let total_target = total_target.max(8).min(300); // 边界保护：最少8张，最多300张

    // 输出容量校验：按模型实际 max_output_tokens 计算能产多少张
    let max_out = ctx_ref.client.max_output_tokens().unwrap_or(8192);
    let output_limit_per_chunk = ((max_out as f64 * 0.8) / 700.0).max(3.0) as usize;
    let total_achievable = total_chunks * output_limit_per_chunk;

    // 取公式目标和输出容量的较小值
    let total_target = total_target.min(total_achievable);

    // 每块目标（向上取整，确保不遗漏）
    let per_chunk_target = (total_target + total_chunks - 1) / total_chunks;
    let per_chunk_target = per_chunk_target.max(8); // 最少8张，避免LLM敷衍

    // 单块卡片软上限：80张 ≈ 一本中等书籍（15万字符）的目标提取量
    // 来源：旧代码分档表中 60K-150K 文档对应 45-80 张，单块信息密度不应超过此量级
    const SOFT_MAX_CARDS_PER_CHUNK: usize = 80;
    let per_chunk_target = per_chunk_target.min(SOFT_MAX_CARDS_PER_CHUNK);

    // card_count_hint：给LLM一个明确的单块目标数字
    let card_count_hint = format!("{}张", per_chunk_target);

    // [S1] Prompt 边界标记：用 XML 标签隔离用户文档，降低 prompt injection 风险
    let prompt = prompt_template
        .replace("{document}", &format!("\n\n<document>\n{}\n</document>\n\n", &document))
        .replace("{card_count_hint}", &card_count_hint)
        .replace("{allowed_types}", allowed_types);

    let system = "你是一位以认知边界爆破与知识最小信息单位为双重信仰的知识炼金术士。你的核心身份是认知牢笼的越狱者、溯源者、连接者和行为改变的系统设计师。所有输出必须严格基于原文，不添加原文没有的信息。".to_string();

    // 调用 JSON mode，同时获取原始文本用于调试保存
    let max_out = ctx_ref.client.max_output_tokens().unwrap_or(8192) as u32;
    let (response_json, raw_text) = ctx_ref
        .call_llm_json_with_raw(
            vec![
                crate::models::LlmMessage {
                    role: "system".to_string(),
                    content: system,
                },
                crate::models::LlmMessage {
                    role: "user".to_string(),
                    content: prompt,
                },
            ],
            Some(max_out),
        )
        .await
        .map_err(|e| {
            AppError::Api(format!("Unified 编译请求失败: {}", e))
        })?;

    // 解析 JSON 响应
    let unified: crate::models::UnifiedChunkResponse = serde_json::from_value(response_json.clone())
        .map_err(|e| {
            AppError::JsonParse(format!(
                "Unified 响应 JSON 解析失败: {}\n原始响应前500字: {}",
                e,
                raw_text.chars().take(500).collect::<String>()
            ))
        })?;

    let (entities, cards) = unified.into_standard_cards();

    let result = ChunkResult {
        document: document.clone(),
        title_path,
        entities,
        cards,
    };

    Ok((result, raw_text))
}

/// 为卡片分配秒级时间戳 UUID（YYYYMMDDHHMMSS）
/// 同一秒内生成多张卡片时，自动 +1 秒保证唯一
/// 传入 base_ts 支持跨块连续分配（避免多块间 UUID 碰撞）
/// 返回最后分配的 ts，供下一块使用
fn assign_unique_ids_with_base(cards: &mut [Card], base_ts: i64) -> i64 {
    let mut last_ts = base_ts;
    for card in cards.iter_mut() {
        let now = chrono::Local::now().timestamp();
        let ts = std::cmp::max(now, last_ts + 1);
        last_ts = ts;
        let dt = chrono::DateTime::from_timestamp(ts, 0)
            .unwrap()
            .with_timezone(&chrono::Local);
        card.unique_id = dt.format("%Y%m%d%H%M%S").to_string();
    }
    last_ts
}

// ── 辅助函数 ──

/// 共享的标题正则（h1-h3），用于语义分块和结构检测
/// 同时被 converter.rs 和 quality 模块引用，避免重复编译
pub(crate) fn heading_regex() -> &'static Regex {
    // [M3] 统一使用 LazyLock（Rust 1.80+ 标准库），与代码库其他位置保持一致
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(#{1,3})\s+(.+)$").expect("硬编码正则应始终有效"));
    &RE
}

/// 提取重叠内容用于下一个 chunk 的上下文
///
/// 策略：保留最后 2000 字符（约一页）+ 当前标题栈，
/// 确保下一个 chunk 有完整的章节上下文，避免边界处信息丢失。
fn extract_overlap(doc: &str, title_stack: &[String]) -> String {
    let overlap_chars = 2000;
    let content = if doc.len() > overlap_chars {
        let raw_start = doc.len().saturating_sub(overlap_chars);
        // 确保 start 落在字符边界上（UTF-8 安全）
        let start = doc
            .char_indices()
            .find(|(i, _)| *i >= raw_start)
            .map(|(i, _)| i)
            .unwrap_or(doc.len());
        // 找到最近的行首，避免截断
        let adjusted_start = doc[start..]
            .find('\n')
            .map(|i| start + i + 1)
            .unwrap_or(start);
        &doc[adjusted_start..]
    } else {
        doc
    };

    // 注入标题上下文
    let header = if title_stack.is_empty() {
        String::new()
    } else {
        title_stack
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{} {}\n", "#".repeat(i + 1), t))
            .collect::<Vec<_>>()
            .join("")
    };

    if header.is_empty() {
        content.to_string()
    } else {
        format!("{}\n{}", header, content)
    }
}

fn flush_chunk(chunks: &mut Vec<(String, String)>, doc: &str, title_stack: &[String]) {
    let title_path = if title_stack.is_empty() {
        String::new()
    } else {
        title_stack.join(" > ")
    };
    let header = if title_path.is_empty() {
        String::new()
    } else {
        format!("# {}\n\n", title_path)
    };
    chunks.push((title_path, header + doc));
}

fn load_prompts_from_dir(dir: &Path) -> HashMap<String, String> {
    let mut prompts = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return prompts;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Ok(content) = std::fs::read_to_string(&path) {
            prompts.insert(stem.to_string(), content);
        }
    }
    prompts
}

fn dedup_cards(cards: &[Card]) -> Vec<Card> {
    crate::dedup::dedup_cards(cards)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CardType;

    #[test]
    fn test_extract_overlap() {
        let doc =
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12";
        let titles = vec!["标题A".to_string()];
        let overlap = extract_overlap(doc, &titles);
        // 重叠包含标题上下文 + 最后约2000字符内容
        assert!(overlap.contains("标题A"));
        assert!(overlap.contains("line12"));
    }

    #[test]
    fn test_extract_overlap_short() {
        let doc = "a\nb";
        let titles: Vec<String> = vec![];
        let overlap = extract_overlap(doc, &titles);
        assert_eq!(overlap, "a\nb");
    }

    #[test]
    fn test_extract_overlap_with_titles() {
        let doc = "content line 1\ncontent line 2";
        let titles = vec!["标题B".to_string(), "标题C".to_string()];
        let overlap = extract_overlap(doc, &titles);
        assert!(overlap.contains("# 标题B"));
        assert!(overlap.contains("## 标题C"));
        assert!(overlap.contains("content line 1"));
    }

    #[test]
    fn test_flush_chunk() {
        let mut chunks = Vec::new();
        flush_chunk(
            &mut chunks,
            "doc content",
            &["标题D".to_string(), "标题E".to_string()],
        );
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, "标题D > 标题E");
        assert!(chunks[0].1.starts_with("# 标题D > 标题E\n\ndoc content"));
    }

    #[test]
    fn test_flush_chunk_empty_stack() {
        let mut chunks = Vec::new();
        flush_chunk(&mut chunks, "doc content", &[]);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, "");
        assert_eq!(chunks[0].1, "doc content");
    }

    #[test]
    fn test_dedup_cards_merge_content() {
        // 语义去重：需要内容高度相似才会合并，而非仅标题相同
        let cards = vec![
            Card {
                title: "标题F".to_string(),
                content: "内容A。".to_string(),
                card_type: CardType::Knowledge,
                reference: "来源A".to_string(),
                unique_id: "20240101120000".to_string(),
                ..Default::default()
            },
            Card {
                title: "标题G".to_string(),
                content: "内容A。".to_string(),
                card_type: CardType::Knowledge,
                reference: "来源B".to_string(),
                unique_id: "20240101120001".to_string(),
                ..Default::default()
            },
            Card {
                title: "标题H".to_string(),
                content: "内容B。".to_string(),
                card_type: CardType::Term,
                reference: "".to_string(),
                unique_id: "20240101120002".to_string(),
                ..Default::default()
            },
        ];
        let result = dedup_cards(&cards);
        // 前两张内容高度相似被合并，第三张独立
        assert_eq!(result.len(), 2);
        // 合并后的卡片包含整合标记
        let merged = result.iter().find(|c| c.title.contains("整合")).unwrap();
        assert!(merged.reference.contains("来源A"));
        assert!(merged.reference.contains("来源B"));
    }

    #[test]
    fn test_dedup_cards_unique() {
        let cards = vec![
            Card {
                title: "卡片A".to_string(),
                content: "内容A".to_string(),
                card_type: CardType::Person,
                reference: "".to_string(),
                unique_id: "".to_string(),
                original_text: "".to_string(),
                source: "".to_string(),
                paraphrase: "".to_string(),
                related_cards: vec![],
                ..Default::default()
            },
            Card {
                title: "卡片B".to_string(),
                content: "内容B".to_string(),
                card_type: CardType::Term,
                reference: "".to_string(),
                unique_id: "".to_string(),
                original_text: "".to_string(),
                source: "".to_string(),
                paraphrase: "".to_string(),
                related_cards: vec![],
                ..Default::default()
            },
        ];
        let result = dedup_cards(&cards);
        assert_eq!(result.len(), 2);
    }

}
