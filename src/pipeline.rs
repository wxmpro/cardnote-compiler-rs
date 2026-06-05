use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use crate::api::LlmClient;
use crate::config::CHUNK_SIZE;
use crate::doc_type::{DocTypeDetector, DocumentType};
use crate::error::{AppError, Result};
use crate::models::{
    Card, ChunkInfo, CompilationDiagnostics, CompilationResult, Entity, KnowledgeGraph, LlmMessage,
    Relation, StageFail, Summary,
};
use crate::stages::cards::{CardPlanner, generate_cards};
use crate::stages::common::{ChatFn, ChatJsonFn};
use crate::stages::entities::{extract_entities, unify_entities};
use crate::stages::graph::build_graph;
use crate::stages::summary::generate_summary;

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
    summary: Summary,
    entities: Vec<Entity>,
    cards: Vec<Card>,
    relations: Vec<Relation>,
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
    pub fn new(client: LlmClient) -> Self {
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

        Self {
            ctx: CompileContext {
                client: Arc::new(client),
                prompts: Arc::new(prompts),
                stage_models: Arc::new(stage_model_map),
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
        let plan_summary = CardPlanner::summary(doc_detection.doc_type, doc_char_count);
        println!("  卡片规划: {}", plan_summary);

        // [H3] 按字符数判断是否需要分块，与 CHUNK_SIZE 的语义（字符数）保持一致
        if document.chars().count() <= CHUNK_SIZE {
            self.run_single(document, source_file, doc_detection.doc_type, book_title)
                .await
        } else {
            self.run_map_reduce(document, source_file, doc_detection.doc_type, book_title)
                .await
        }
    }

    /// 仅运行 AI 摘要
    pub async fn run_summary(&self, document: &str) -> Result<Summary> {
        println!("\n[阶段] AI 摘要");
        println!("{}", "-".repeat(60));
        let ctx = &self.ctx;
        let result = generate_summary(document, ctx, &|name| ctx.load_prompt(name)).await?;
        println!("  ✓ 标题: {}", result.title);
        Ok(result)
    }

    /// 仅运行 AI 标注
    pub async fn run_entities(&self, document: &str) -> Result<Vec<Entity>> {
        println!("\n[阶段] AI 标注 — 实体识别");
        println!("{}", "-".repeat(60));
        let ctx = &self.ctx;
        let result = extract_entities(document, ctx, ctx, &|name| ctx.load_prompt(name)).await?;
        println!("  ✓ 识别 {} 个实体", result.len());
        Ok(result)
    }

    /// 仅运行 AI 卡片
    pub async fn run_cards(&self, document: &str, book_title: &str) -> Result<Vec<Card>> {
        println!("\n[阶段] AI 卡片 — 10 种卡片类型");
        println!("{}", "-".repeat(60));
        let ctx = &self.ctx;
        let doc_type = DocTypeDetector::detect(document);
        let result = generate_cards(document, doc_type, book_title, ctx, &|name| {
            ctx.load_prompt(name)
        })
        .await?;
        println!("  ✓ 生成 {} 张卡片", result.len());
        Ok(result)
    }

    /// 仅运行 AI 图谱
    pub async fn run_graph(&self, document: &str, entities: &[Entity]) -> Result<KnowledgeGraph> {
        println!("\n[阶段] AI 图谱 — 知识关系网络");
        println!("{}", "-".repeat(60));
        let ctx = &self.ctx;
        let result =
            build_graph(document, entities, ctx, ctx, &|name| ctx.load_prompt(name)).await?;
        println!("  ✓ {} 条关系", result.relations.len());
        Ok(result)
    }

    /// 多文档策展编译
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

        let result = match with_retry("unified", || {
            compile_chunk_unified(
                ctx.clone(),
                document.to_string(),
                String::new(),
                doc_type,
                book_title.to_string(),
            )
        })
        .await
        {
            Ok(r) => r,
            Err(e) => {
                let err_msg = format!("{}", e);
                println!("  ✗ Unified 编译失败: {}", err_msg);
                diagnostics.failures.push(StageFail {
                    stage: "unified".to_string(),
                    error: err_msg,
                    retry_count: 3,
                    final_status: "failed".to_string(),
                });
                ChunkResult::default()
            }
        };

        println!("\n{}", "=".repeat(60));
        println!("编译完成！");
        println!(
            "  实体: {} 个  |  卡片: {} 张",
            result.entities.len(),
            result.cards.len()
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
            summary: result.summary,
            cards: result.cards,
            graph: KnowledgeGraph {
                entities: result.entities.clone(),
                relations: result.relations,
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
        println!("\n[模式] 分块 Map-Reduce 编译（阈值: {} 字符）", CHUNK_SIZE);
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

        // 2b. 编译未完成的块（串行：一次一个请求，避免 API 429）
        let semaphore = Arc::new(Semaphore::new(1));
        let mut handles = Vec::new();
        for idx in &pending {
            let idx = *idx; // copy to avoid borrow issue
            let permit = semaphore
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| AppError::TaskPanic(format!("并发信号量获取失败: {}", e)))?;
            let ctx = self.ctx.clone();
            let (title, doc) = chunks[idx].clone();
            println!("  处理块 {}/{}...", idx + 1, chunks_len);
            let bt = book_title.to_string();
            handles.push(tokio::spawn(async move {
                let _permit = permit;
                let result = compile_chunk_unified(ctx, doc, title, doc_type, bt).await;
                (idx, result)
            }));
        }
        for handle in handles {
            let (idx, result) = handle
                .await
                .map_err(|e| AppError::TaskPanic(format!("并发任务 panic: {}", e)))?;
            match result {
                Ok(result) => {
                    println!(
                        "  ✓ 块 {}/{} 完成 — 实体 {} 个 | 卡片 {} 张",
                        idx + 1,
                        chunks_len,
                        result.entities.len(),
                        result.cards.len()
                    );
                    chunk_results[idx] = result.clone();
                    cache.set_chunk_result(idx, result);
                    cache.save(source_file, &provider, &model, document).ok();
                }
                Err(e) => {
                    let err_msg = format!("块 {} 编译失败: {}", idx + 1, e);
                    println!("  ✗ {}", err_msg);
                    failed_chunks.push((idx, err_msg));
                }
            }
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
                    match compile_chunk_unified(
                        ctx,
                        doc.clone(),
                        title.clone(),
                        doc_type,
                        book_title.to_string(),
                    )
                    .await
                    {
                        Ok(result) => {
                            println!(
                                "  ✓ 块 {}/{} 重试成功 — 实体 {} 个 | 卡片 {} 张",
                                idx + 1,
                                chunks_len,
                                result.entities.len(),
                                result.cards.len()
                            );
                            chunk_results[*idx] = result;
                        }
                        Err(e) => {
                            let err_msg = format!("块 {} 重试失败: {}", idx + 1, e);
                            println!("  ✗ {}", err_msg);
                            still_failed.push((*idx, err_msg));
                        }
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

                if current_size >= CHUNK_SIZE - 5000 {
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

            if current_size >= CHUNK_SIZE {
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

/// Unified 编译模式：一次 LLM 请求生成 summary + entities + cards + relations
async fn compile_chunk_unified(
    ctx: CompileContext,
    document: String,
    title_path: String,
    _doc_type: DocumentType,
    _book_title: String,
) -> Result<ChunkResult> {
    let ctx_ref = &ctx;

    // 加载 unified prompt
    let prompt_template = ctx_ref.load_prompt("unified")?;
    let prompt = prompt_template.replace("{document}", &document);

    let system = "你是一位资深的内容分析师、知识策展人和知识图谱专家。你的核心能力是从复杂文本中同时完成：结构化摘要提取、实体识别、知识卡片生成和关系图谱构建。所有输出必须严格基于原文，不添加原文没有的信息。".to_string();

    // 调用 JSON mode
    let response = ctx_ref
        .call_llm_json(
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
            Some(32768), // v4-pro 单次输出上限 384K，给足空间避免截断
        )
        .await
        .map_err(|e| {
            AppError::Api(format!("Unified 编译请求失败: {}", e))
        })?;

    // 解析 JSON 响应
    let unified: crate::models::UnifiedChunkResponse = serde_json::from_value(response.clone())
        .map_err(|e| {
            AppError::JsonParse(format!(
                "Unified 响应 JSON 解析失败: {}\n原始响应前500字: {}",
                e,
                response.to_string().chars().take(500).collect::<String>()
            ))
        })?;

    let (entities, cards) = unified.into_standard_cards();

    Ok(ChunkResult {
        document: document.clone(),
        title_path,
        summary: Summary::default(),
        entities,
        cards,
        relations: Vec::new(),
    })
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

    #[test]
    fn test_merge_doc_summaries_empty() {
        let result = Pipeline::merge_doc_summaries(&[]);
        assert_eq!(result.title, "");
        assert!(result.key_points.is_empty());
    }

    #[test]
    fn test_merge_doc_summaries_single() {
        let summaries = vec![Summary {
            title: "标题".to_string(),
            overview: "概述".to_string(),
            key_points: vec!["要点1".to_string()],
            structure: "结构".to_string(),
        }];
        let result = Pipeline::merge_doc_summaries(&summaries);
        assert_eq!(result.title, "标题");
        assert_eq!(result.overview, "概述");
        assert_eq!(result.key_points.len(), 1);
        assert_eq!(result.structure, "结构");
    }

    #[test]
    fn test_merge_doc_summaries_multiple() {
        let summaries = vec![
            Summary {
                title: "文档A".to_string(),
                overview: "概述A".to_string(),
                key_points: vec!["A1".to_string(), "A2".to_string()],
                structure: "结构A".to_string(),
            },
            Summary {
                title: "文档B".to_string(),
                overview: "概述B".to_string(),
                key_points: vec!["B1".to_string()],
                structure: "".to_string(),
            },
        ];
        let result = Pipeline::merge_doc_summaries(&summaries);
        assert_eq!(result.title, "文档A");
        assert!(result.overview.contains("概述A"));
        assert!(result.overview.contains("概述B"));
        assert_eq!(result.key_points.len(), 3);
        assert_eq!(result.structure, "结构A");
    }

    #[test]
    fn test_merge_doc_summaries_points_limit() {
        let mut summaries = Vec::new();
        for i in 0..15 {
            summaries.push(Summary {
                title: format!("文档{}", i),
                overview: "".to_string(),
                key_points: vec!["要点".to_string(); 2],
                structure: "".to_string(),
            });
        }
        let result = Pipeline::merge_doc_summaries(&summaries);
        assert_eq!(result.key_points.len(), 20);
    }
}
