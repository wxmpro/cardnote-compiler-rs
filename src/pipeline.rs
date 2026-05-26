use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::api::LlmClient;
use crate::config::{CHUNK_SIZE, MAX_WORKERS};
use crate::doc_type::{DocTypeDetector, DocumentType};
use crate::error::{AppError, Result};
use crate::models::{
    Card, ChunkInfo, CompilationDiagnostics, CompilationResult, Document, Entity, KnowledgeGraph,
    LlmMessage, Relation, StageFail, Summary,
};
use crate::output;
use crate::stages::cards::{CardPlanner, generate_cards};
use crate::stages::common::{ChatFn, ChatJsonFn};
use crate::stages::entities::{extract_entities, unify_entities};
use crate::stages::graph::{build_graph, merge_relations, update_relation_endpoints};
use crate::stages::summary::{generate_summary, merge_summaries};

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
    const CURRENT_VERSION: u32 = 1;
    const CACHE_DIR: &'static str = ".cardc_cache";

    /// 计算文档哈希
    fn hash_document(document: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        document.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// 获取缓存文件路径
    fn cache_path(source_file: &str) -> PathBuf {
        let cache_dir = Path::new(Self::CACHE_DIR);
        std::fs::create_dir_all(cache_dir).ok();
        let filename = format!("{}.cache.json", source_file.replace(['/', '\\', ':'], "_"));
        cache_dir.join(filename)
    }

    /// 加载缓存
    fn load(source_file: &str, document: &str) -> Option<Self> {
        let path = Self::cache_path(source_file);
        if !path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(&path).ok()?;
        let cache: CompileCache = serde_json::from_str(&content).ok()?;

        // 验证版本和文档哈希
        let expected_hash = Self::hash_document(document);
        if cache.version != Self::CURRENT_VERSION || cache.document_hash != expected_hash {
            return None;
        }

        Some(cache)
    }

    /// 保存缓存
    fn save(&self, source_file: &str) -> Result<()> {
        let path = Self::cache_path(source_file);
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| AppError::TaskPanic(format!("缓存序列化失败: {}", e)))?;
        std::fs::write(&path, content)
            .map_err(|e| AppError::TaskPanic(format!("缓存写入失败: {}", e)))?;
        Ok(())
    }

    /// 创建新缓存
    fn new(document: &str, chunk_count: usize) -> Self {
        Self {
            document_hash: Self::hash_document(document),
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

/// 编译上下文（可 Clone，用于并行任务）
#[derive(Clone, Debug)]
struct CompileContext {
    client: Arc<LlmClient>,
    prompts: Arc<HashMap<String, String>>,
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
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()));
        let candidates: Vec<PathBuf> = if let Some(ref dir) = exe_dir {
            vec![
                dir.join("../prompts"),             // exe 旁（如 bin/）
                dir.join("../../prompts"),          // target/release/ 等深层目录
                dir.join("../../../prompts"),       // 更深一层
                Path::new("prompts").to_path_buf(), // 运行时工作目录
            ]
        } else {
            vec![Path::new("prompts").to_path_buf()]
        };

        for dir in candidates {
            prompts = load_prompts_from_dir(&dir);
            if !prompts.is_empty() {
                break;
            }
        }

        Self {
            ctx: CompileContext {
                client: Arc::new(client),
                prompts: Arc::new(prompts),
            },
        }
    }

    /// 运行完整编译流程
    pub async fn run(&self, document: &str, source_file: &str) -> Result<CompilationResult> {
        println!("{}", "=".repeat(60));
        println!("知识编译引擎启动");
        println!("{}", "=".repeat(60));
        println!("  文档大小: {} 字符", document.len());
        let doc_detection = DocTypeDetector::detect_with_confidence(document);
        let plan_summary = CardPlanner::summary(doc_detection.doc_type, document.len());
        println!("  卡片规划: {}", plan_summary);

        if document.len() <= CHUNK_SIZE {
            self.run_single(document, source_file, doc_detection.doc_type)
                .await
        } else {
            self.run_map_reduce(document, source_file, doc_detection.doc_type)
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
    pub async fn run_cards(&self, document: &str) -> Result<Vec<Card>> {
        println!("\n[阶段] AI 卡片 — 10 种卡片类型");
        println!("{}", "-".repeat(60));
        let ctx = &self.ctx;
        let doc_type = DocTypeDetector::detect(document);
        let result = generate_cards(document, doc_type, ctx, &|name| ctx.load_prompt(name)).await?;
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
    pub async fn compile_book(&self, documents: Vec<Document>, book_title: &str) -> Result<String> {
        println!("{}", "=".repeat(60));
        println!("多文档知识编译启动");
        println!("{}", "=".repeat(60));

        let mut all_results = Vec::new();
        let mut all_cards = Vec::new();
        let mut all_entities = Vec::new();
        let mut all_relations = Vec::new();

        for (i, doc) in documents.iter().enumerate() {
            println!("\n[文档 {}/{}] {}", i + 1, documents.len(), doc.title);
            let result = self.run(&doc.content, &doc.source_file).await?;
            let entity_count = result.graph.entities.len();
            let card_count = result.cards.len();
            println!("  ✓ 实体 {} 个 | 卡片 {} 张", entity_count, card_count);
            all_results.push(result.clone());
            all_cards.extend(result.cards);
            all_entities.extend(result.graph.entities);
            all_relations.extend(result.graph.relations);
        }

        // 质量过滤：移除空卡片/低质量卡片
        let joined_document = documents
            .iter()
            .map(|doc| doc.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let (filtered_cards, _lint_stats) = crate::quality::filter_cards_with_source(
            &all_cards,
            &joined_document,
            &crate::quality::CardLintConfig::default(),
        );

        let (unique_entities, entity_stats, entity_name_map) = unify_entities(&all_entities);
        if entity_stats.merged_groups > 0 {
            eprintln!(
                "  ✓ 实体统一: {} 个实体 → {} 个 (合并 {} 组, 消除 {} 个重复)",
                entity_stats.original_count,
                entity_stats.unified_count,
                entity_stats.merged_groups,
                entity_stats.eliminated_duplicates
            );
        }
        let unique_cards = dedup_cards(&filtered_cards);
        let updated_relations = update_relation_endpoints(&all_relations, &entity_name_map);
        let unique_relations = merge_relations(&updated_relations);

        let cross_relations = self
            .discover_cross_relations(&unique_entities, &documents, &entity_name_map)
            .await?;

        let global_summary = Self::merge_doc_summaries(
            &all_results
                .iter()
                .map(|r| r.summary.clone())
                .collect::<Vec<_>>(),
        );

        println!(
            "\n  总计: 实体 {} 个 | 卡片 {} 张 | 关系 {} 条",
            unique_entities.len(),
            unique_cards.len(),
            unique_relations.len()
        );
        println!("  跨文档关系: {} 条", cross_relations.len());

        let curation_data = output::CurationData {
            global_summary: &global_summary,
            cards: &unique_cards,
            entities: &unique_entities,
            relations: &unique_relations,
            cross_relations: &cross_relations,
            doc_results: &all_results,
        };
        let output_path = output::save_curation(&curation_data, "./output", book_title).await?;

        println!("\n结果已保存到: {}", output_path);
        Ok(output_path)
    }

    // ── 单轮编译 ──

    async fn run_single(
        &self,
        document: &str,
        source_file: &str,
        doc_type: DocumentType,
    ) -> Result<CompilationResult> {
        println!("\n[模式] 单轮全量编译");
        println!("{}", "-".repeat(60));

        let mut diagnostics = CompilationDiagnostics::default();
        let ctx = &self.ctx;

        // 摘要阶段
        let summary = match generate_summary(document, ctx, &|name| ctx.load_prompt(name)).await {
            Ok(s) => {
                println!("  ✓ 摘要完成");
                s
            }
            Err(e) => {
                let err_msg = format!("{}", e);
                println!("  ✗ 摘要失败: {}", err_msg);
                diagnostics.failures.push(StageFail {
                    stage: "summary".to_string(),
                    error: err_msg,
                    retry_count: 0,
                    final_status: "failed".to_string(),
                });
                Summary::default()
            }
        };

        // 实体阶段
        let entities =
            match extract_entities(document, ctx, ctx, &|name| ctx.load_prompt(name)).await {
                Ok(e) => {
                    println!("  ✓ 标注完成 — 识别 {} 个实体", e.len());
                    e
                }
                Err(e) => {
                    let err_msg = format!("{}", e);
                    println!("  ✗ 实体提取失败: {}", err_msg);
                    diagnostics.failures.push(StageFail {
                        stage: "entities".to_string(),
                        error: err_msg,
                        retry_count: 0,
                        final_status: "failed".to_string(),
                    });
                    Vec::new()
                }
            };

        // 卡片阶段
        let cards =
            match generate_cards(document, doc_type, ctx, &|name| ctx.load_prompt(name)).await {
                Ok(c) => {
                    println!("  ✓ 卡片完成 — 生成 {} 张卡片", c.len());
                    c
                }
                Err(e) => {
                    let err_msg = format!("{}", e);
                    println!("  ✗ 卡片生成失败: {}", err_msg);
                    diagnostics.failures.push(StageFail {
                        stage: "cards".to_string(),
                        error: err_msg,
                        retry_count: 0,
                        final_status: "failed".to_string(),
                    });
                    Vec::new()
                }
            };

        // 图谱阶段
        let graph =
            match build_graph(document, &entities, ctx, ctx, &|name| ctx.load_prompt(name)).await {
                Ok(g) => {
                    println!("  ✓ 图谱完成 — {} 条关系", g.relations.len());
                    g
                }
                Err(e) => {
                    let err_msg = format!("{}", e);
                    println!("  ✗ 图谱构建失败: {}", err_msg);
                    diagnostics.failures.push(StageFail {
                        stage: "graph".to_string(),
                        error: err_msg,
                        retry_count: 0,
                        final_status: "failed".to_string(),
                    });
                    KnowledgeGraph {
                        entities: Vec::new(),
                        relations: Vec::new(),
                    }
                }
            };

        println!("\n{}", "=".repeat(60));
        println!("编译完成！");
        println!(
            "  实体: {} 个  |  卡片: {} 张  |  关系: {} 条",
            entities.len(),
            cards.len(),
            graph.relations.len()
        );
        if !diagnostics.failures.is_empty() {
            println!(
                "  ⚠ {} 个阶段失败，详见诊断报告",
                diagnostics.failures.len()
            );
        }
        println!("{}", "=".repeat(60));

        Ok(CompilationResult {
            source_file: source_file.to_string(),
            summary,
            cards,
            graph,
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
    ) -> Result<CompilationResult> {
        println!(
            "\n[模式] 分块 Map-Reduce 编译（阈值: {} 字符，并行: {} 任务）",
            CHUNK_SIZE, MAX_WORKERS
        );
        println!("{}", "-".repeat(60));

        // 1. Split
        println!("\n[1/4] 语义分块...");
        let chunks = self.semantic_chunk(document);
        let chunks_len = chunks.len();
        println!("  分成 {} 个语义块", chunks_len);

        // 2. Map — 串行编译（支持断点续编译）
        println!("\n[2/4] Map 阶段 — 串行编译...");
        let mut chunk_results: Vec<ChunkResult> = vec![ChunkResult::default(); chunks_len];
        let mut failed_chunks: Vec<(usize, String)> = Vec::new();

        // 2a. 检查编译缓存（断点续编译）
        let mut cache = CompileCache::load(source_file, document)
            .unwrap_or_else(|| CompileCache::new(document, chunks_len));
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

        // 2b. 编译未完成的块
        for idx in &pending {
            let (title, doc) = chunks[*idx].clone();
            println!("  处理块 {}/{}...", idx + 1, chunks_len);
            let ctx = self.ctx.clone();
            match compile_chunk(ctx, doc, title, doc_type).await {
                Ok(result) => {
                    println!(
                        "  ✓ 块 {}/{} 完成 — 实体 {} 个 | 卡片 {} 张",
                        idx + 1,
                        chunks_len,
                        result.entities.len(),
                        result.cards.len()
                    );
                    chunk_results[*idx] = result.clone();
                    cache.set_chunk_result(*idx, result);
                    cache.save(source_file).ok();
                }
                Err(e) => {
                    let err_msg = format!("块 {} 编译失败: {}", idx + 1, e);
                    println!("  ✗ {}", err_msg);
                    failed_chunks.push((*idx, err_msg));
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
                    match compile_chunk(ctx, doc.clone(), title.clone(), doc_type).await {
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
        println!("\n[3/4] Reduce 阶段 — 合并去重...");
        let mut all_entities = Vec::new();
        let mut all_cards = Vec::new();
        let mut all_relations = Vec::new();
        let mut all_summaries = Vec::new();

        // 先生成 ChunkInfo（在 move 数据之前），避免后续 clone
        let chunk_infos: Vec<ChunkInfo> = chunk_results
            .iter()
            .map(|c| ChunkInfo {
                title_path: c.title_path.clone(),
                size: c.document.len(),
                entities: c.entities.len(),
                cards: c.cards.len(),
                relations: c.relations.len(),
            })
            .collect();

        for r in &mut chunk_results {
            all_entities.append(&mut r.entities);
            all_cards.append(&mut r.cards);
            all_relations.append(&mut r.relations);
            if !r.summary.title.is_empty() || !r.summary.overview.is_empty() {
                all_summaries.push(std::mem::take(&mut r.summary));
            }
        }

        // 质量过滤：移除空卡片/低质量卡片
        let (filtered_cards, _lint_stats) = crate::quality::filter_cards_with_source(
            &all_cards,
            document,
            &crate::quality::CardLintConfig::default(),
        );

        let (unique_entities, entity_stats, entity_name_map) = unify_entities(&all_entities);
        if entity_stats.merged_groups > 0 {
            eprintln!(
                "  ✓ 实体统一: {} 个实体 → {} 个 (合并 {} 组, 消除 {} 个重复)",
                entity_stats.original_count,
                entity_stats.unified_count,
                entity_stats.merged_groups,
                entity_stats.eliminated_duplicates
            );
        }
        let unique_cards = dedup_cards(&filtered_cards);
        let updated_relations = update_relation_endpoints(&all_relations, &entity_name_map);
        let unique_relations = merge_relations(&updated_relations);

        println!(
            "  合并后: 实体 {} 个 | 卡片 {} 张 | 关系 {} 条",
            unique_entities.len(),
            unique_cards.len(),
            unique_relations.len()
        );

        // 4. 全局摘要
        println!("\n[4/4] 生成全局摘要...");
        let ctx = &self.ctx;
        let global_summary =
            match merge_summaries(&all_summaries, document, ctx, &|name| ctx.load_prompt(name))
                .await
            {
                Ok(s) => {
                    println!("  ✓ 全局摘要完成");
                    s
                }
                Err(_) => {
                    println!("  ⚠ 全局摘要生成失败，使用本地合并");
                    Pipeline::merge_doc_summaries(&all_summaries)
                }
            };

        println!("\n{}", "=".repeat(60));
        println!("分块编译完成！");
        println!("  分块数: {}", chunk_results.len());
        println!(
            "  实体: {} 个  |  卡片: {} 张  |  关系: {} 条",
            unique_entities.len(),
            unique_cards.len(),
            unique_relations.len()
        );
        println!("{}", "=".repeat(60));

        Ok(CompilationResult {
            source_file: source_file.to_string(),
            summary: global_summary,
            cards: unique_cards,
            graph: KnowledgeGraph {
                entities: unique_entities,
                relations: unique_relations,
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

    // ── 工具方法 ──

    fn merge_doc_summaries(summaries: &[Summary]) -> Summary {
        if summaries.is_empty() {
            return Summary::default();
        }

        let all_points: Vec<String> = summaries
            .iter()
            .flat_map(|s| s.key_points.clone())
            .take(20)
            .collect();

        Summary {
            title: summaries
                .first()
                .map(|s| s.title.clone())
                .unwrap_or_default(),
            overview: summaries
                .iter()
                .filter_map(|s| {
                    if s.overview.is_empty() {
                        None
                    } else {
                        Some(s.overview.clone())
                    }
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
            key_points: all_points,
            structure: summaries
                .iter()
                .filter_map(|s| {
                    if s.structure.is_empty() {
                        None
                    } else {
                        Some(s.structure.clone())
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    async fn discover_cross_relations(
        &self,
        entities: &[Entity],
        documents: &[Document],
        entity_name_map: &std::collections::HashMap<String, String>,
    ) -> Result<Vec<Relation>> {
        if documents.len() < 2 || entities.len() < 2 {
            return Ok(Vec::new());
        }

        // 构建反向名称映射：统一名称 → 所有原始名称（含自身）
        // 用于在文档内容中搜索时匹配所有名称变体
        let mut name_aliases: std::collections::HashMap<String, std::collections::HashSet<String>> =
            std::collections::HashMap::new();
        for (original, unified) in entity_name_map {
            name_aliases
                .entry(unified.clone())
                .or_default()
                .insert(original.clone());
            name_aliases
                .entry(unified.clone())
                .or_default()
                .insert(unified.clone());
        }
        // 未在映射中出现的实体（本身已是统一名称）也加入别名集合
        for e in entities {
            name_aliases
                .entry(e.name.clone())
                .or_default()
                .insert(e.name.clone());
        }

        let prompt = self.ctx.load_prompt("cross_document")?;

        let doc_texts: Vec<String> = documents
            .iter()
            .map(|d| {
                let text: String = d.content.chars().take(2000).collect();
                let doc_entities: Vec<String> = entities
                    .iter()
                    .filter(|e| {
                        // 使用别名集合匹配：统一名称或其任何原始变体
                        let aliases = name_aliases.get(&e.name).cloned().unwrap_or_default();
                        aliases.iter().any(|alias| text.contains(alias))
                    })
                    .map(|e| e.name.clone())
                    .take(10)
                    .collect();
                format!("文档《{}》：涉及实体 {}", d.title, doc_entities.join("、"))
            })
            .collect();

        let entities_text: Vec<String> = entities
            .iter()
            .take(20)
            .map(|e| format!("- {} ({})", e.name, e.entity_type))
            .collect();

        let user = prompt
            .replace("{documents}", &doc_texts.join("\n\n"))
            .replace("{entities}", &entities_text.join("\n"));

        let response = self
            .ctx
            .call_llm(
                vec![
                    LlmMessage {
                        role: "system".to_string(),
                        content: "你是一位知识策展人，擅长发现不同文档之间的隐性关联。".to_string(),
                    },
                    LlmMessage {
                        role: "user".to_string(),
                        content: user,
                    },
                ],
                Some(8000),
            )
            .await?;

        // 解析 |source|relation|target|evidence| 格式
        let mut relations = Vec::new();
        for line in response.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.splitn(4, '|').collect();
            if parts.len() >= 3 {
                relations.push(Relation {
                    source: parts[0].trim().to_string(),
                    relation_type: parts[1].trim().to_string(),
                    target: parts[2].trim().to_string(),
                    evidence: parts
                        .get(3)
                        .map(|s| s.trim().to_string())
                        .unwrap_or_default(),
                });
            }
        }

        Ok(relations)
    }
}

/// 编译单个语义块（独立函数，可被并行调用）
async fn compile_chunk(
    ctx: CompileContext,
    document: String,
    title_path: String,
    doc_type: DocumentType,
) -> Result<ChunkResult> {
    let ctx_ref = &ctx;
    let load_prompt = |name: &str| ctx_ref.load_prompt(name);

    // 辅助函数：带重试的执行（3次重试 + 指数退避）
    async fn with_retry<T, F, Fut>(name: &str, f: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let max_retries = 3;
        let mut last_err = None;
        for attempt in 1..=max_retries {
            match f().await {
                Ok(result) => return Ok(result),
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

    // summary / entities / cards 串行执行，避免并发 API 请求过多触发限流
    // 对每个阶段的失败进行容错处理：失败时使用默认值并继续
    let summary = match with_retry("摘要", || {
        generate_summary(&document, ctx_ref, &load_prompt)
    })
    .await
    {
        Ok(s) => s,
        Err(_) => Summary::default(),
    };

    let entities = match with_retry("实体", || {
        extract_entities(&document, ctx_ref, ctx_ref, &load_prompt)
    })
    .await
    {
        Ok(e) => e,
        Err(_) => Vec::new(),
    };

    let cards = match with_retry("卡片", || {
        generate_cards(&document, doc_type, ctx_ref, &load_prompt)
    })
    .await
    {
        Ok(c) => c,
        Err(_) => Vec::new(),
    };

    // graph 依赖 entities 结果，顺序执行
    let graph = match with_retry("图谱", || {
        build_graph(&document, &entities, ctx_ref, ctx_ref, &load_prompt)
    })
    .await
    {
        Ok(g) => g,
        Err(_) => KnowledgeGraph {
            entities: Vec::new(),
            relations: Vec::new(),
        },
    };

    Ok(ChunkResult {
        document: document.clone(),
        title_path,
        summary,
        entities,
        cards,
        relations: graph.relations,
    })
}

// ── 辅助函数 ──

fn heading_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(#{1,3})\s+(.+)$").expect("硬编码正则应始终有效"))
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
