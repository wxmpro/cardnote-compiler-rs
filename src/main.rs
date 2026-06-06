use std::path::Path;

use chrono::Local;
use clap::{Parser, Subcommand};
use colored::*;

use cardnote_compiler::api::create_client;
use cardnote_compiler::config::{find_env_file, get_api_config, get_provider_label};
use cardnote_compiler::converter::{
    convert_to_markdown_async, convert_to_markdown_async_with_timeout, extract_pdf_metadata,
};
use cardnote_compiler::diagnostics;
use cardnote_compiler::error::AppError;
use cardnote_compiler::health::{check_all_providers, print_health_report, select_best};
use cardnote_compiler::pipeline::Pipeline;
use cardnote_compiler::quality;
use cardnote_compiler::scan;
use cardnote_compiler::scan::PdfStatus;

/// 启动时清理超过 24 小时的残留临时目录
fn cleanup_stale_temp_dirs() {
    let temp_base = std::env::temp_dir();
    let one_day_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(86400);

    if let Ok(entries) = std::fs::read_dir(&temp_base) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // tempfile crate 默认命名: .tmpXXXXXX
            if (name.starts_with(".tmp") || name.starts_with("cardnote_"))
                && let Ok(metadata) = entry.metadata()
                && let Ok(modified) = metadata.modified()
                && modified < one_day_ago
            {
                if let Err(e) = std::fs::remove_dir_all(&path) {
                    eprintln!("  ⚠ 清理残留临时目录失败 {}: {}", path.display(), e);
                }
            }
        }
    }
}

#[derive(Parser)]
#[command(name = "cardc")]
#[command(about = "CardNote Compiler — 把文档编译成知识卡片 (Rust 版)")]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// 输入文件路径
    file: Option<String>,

    /// 输出目录
    #[arg(short, long, default_value = "./output")]
    output: String,

    /// LLM 提供商
    #[arg(long)]
    provider: Option<String>,

    /// 模型名称
    #[arg(long)]
    model: Option<String>,

    /// API Key（⚠️ 会留在 shell history 中，建议优先使用环境变量 LLM_API_KEY）
    #[arg(long)]
    api_key: Option<String>,

    /// 自定义 API 地址
    #[arg(long)]
    base_url: Option<String>,

    /// 低质量输入仍强制继续编译
    #[arg(long)]
    force: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// 重新配置 API
    Init,
    /// 环境诊断
    Doctor,
    /// 检测 PDF 解析质量
    Quality {
        /// 输入文件
        file: String,
    },
    /// 扫描目录下的 PDF，检测哪些需要 OCR
    Scan {
        /// 输入目录
        dir: String,
        /// 递归扫描子目录
        #[arg(short, long)]
        recursive: bool,
        /// 文字层判定阈值（字符数）
        #[arg(short, long, default_value = "20")]
        threshold: usize,
    },
    /// 查看历史编译记录
    History {
        /// 显示最近 N 条记录（默认 20）
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
    /// 标记编译记录为已审阅
    Review {
        /// 编译记录 ID（从 cardc history 获取）
        id: i64,
    },
}

#[tokio::main]
async fn main() {
    // 启动时清理残留临时目录
    cleanup_stale_temp_dirs();

    if let Err(e) = run().await {
        eprintln!("{} {}", "错误:".red().bold(), e);
        std::process::exit(1);
    }
}

// [H5] 主调度函数：职责仅为解析 CLI 并路由到对应处理器
async fn run() -> cardnote_compiler::error::Result<()> {
    // 加载 .env
    if let Some(path) = find_env_file()
        && let Err(e) = dotenvy::from_path(&path)
    {
        eprintln!("{} 加载 .env 文件失败: {}", "⚠".yellow(), e);
    }

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init) => {
            cardnote_compiler::config::interactive_setup().await?;
            Ok(())
        }
        Some(Commands::Doctor) => {
            diagnostics::cmd_doctor().await?;
            Ok(())
        }
        Some(Commands::Quality { file }) => handle_quality(&file).await,
        Some(Commands::Scan {
            dir,
            recursive,
            threshold,
        }) => handle_scan(&dir, recursive, threshold).await,
        Some(Commands::History { limit }) => handle_history(limit).await,
        Some(Commands::Review { id }) => handle_review(id).await,
        None => handle_compile(cli).await,
    }
}

// [H5] Quality 命令处理器
async fn handle_quality(file: &str) -> cardnote_compiler::error::Result<()> {
    println!("读取文件: {}", file);
    let document = match convert_to_markdown_async_with_timeout(file, Some(60)).await {
        Ok(doc) => doc,
        Err(AppError::Timeout(msg)) => {
            println!("  {} {}", "⚠".yellow(), msg);
            println!(
                "\n  {} 该 PDF 可能是扫描版/图片版，OCR 处理耗时较长。",
                "提示:".yellow().bold()
            );
            println!("  如需完整质量检测，请确保 OCR 工具可用，或使用文本版 PDF。\n");
            let placeholder = format!(
                "# 扫描版 PDF\n\n该文件 [{}] 被识别为扫描版/图片版 PDF。\n需要 OCR 工具提取文本内容。",
                file
            );
            let report = quality::analyze(&placeholder);
            quality::print_report(&report);
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    println!("  {} {} 字符", "✓".green(), document.chars().count());
    let report = quality::analyze(&document);
    quality::print_report(&report);
    Ok(())
}

// [H5] Scan 命令处理器
async fn handle_scan(
    dir: &str,
    recursive: bool,
    threshold: usize,
) -> cardnote_compiler::error::Result<()> {
    let report = scan::scan_directory_async(dir, recursive, threshold)
        .await
        .map_err(|e| AppError::TaskPanic(format!("扫描失败: {}", e)))?;
    scan::print_scan_report(&report);
    Ok(())
}

/// 从文件路径和 PDF 元数据解析书名
fn resolve_book_title(file: &str, is_pdf: bool) -> String {
    if is_pdf {
        let meta = extract_pdf_metadata(file);
        if !meta.title.is_empty() {
            return meta.title;
        }
    }
    Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("未命名")
        .to_string()
}

// [H5] 主编译流程处理器（原 run() 中 None 分支的核心逻辑）
async fn handle_compile(cli: Cli) -> cardnote_compiler::error::Result<()> {
    let file = cli.file.ok_or_else(|| {
        AppError::Config("请指定输入文件，或运行 cardc init / cardc doctor".to_string())
    })?;

    // 路径安全验证：规范化路径（消除 .. 遍历和符号链接）
    if file.contains('\0') || file.contains("..") {
        return Err(AppError::FileNotFound(
            "路径包含非法字符或路径遍历尝试".to_string(),
        ));
    }
    let file = std::fs::canonicalize(&file)
        .map_err(|e| AppError::FileNotFound(format!("路径规范化失败: {}", e)))?
        .to_string_lossy()
        .to_string();

    // ── 自动 Provider 健康检测与选择 ──
    let (api_key, provider, model) = if cli.api_key.is_none() && cli.provider.is_none() {
        println!("{} 正在检测所有 Provider 连通性...", "🔍".bright_yellow());
        let health_results = check_all_providers().await;
        print_health_report(&health_results);

        if let Some((best_provider_id, best_model, best_label)) = select_best(&health_results) {
            println!(
                "\n{} 自动选择最佳 Provider: {} ({})",
                "✓".green(),
                best_label.bright_cyan(),
                format!("{}ms", health_results[0].latency_ms).bright_black()
            );
            let credentials = cardnote_compiler::providers::scan_credentials();
            let cred = credentials.get(&best_provider_id).ok_or_else(|| {
                AppError::Config(format!("最佳 Provider {} 凭据丢失", best_provider_id))
            })?;
            (cred.api_key.clone(), best_provider_id, Some(best_model))
        } else {
            println!("\n{} 没有可用的 Provider，启动交互式配置...", "⚠".yellow());
            let (key, prov, model, _) = cardnote_compiler::config::interactive_setup().await?;
            (key, prov, model)
        }
    } else {
        let (key, prov) = get_api_config(cli.api_key, cli.provider).await?;
        (key, prov, cli.model)
    };

    let client = create_client(&provider, &api_key, model, cli.base_url)?;

    let provider_label = get_provider_label(&provider);
    let count = file.chars().count();
    let file_display = if count < 60 {
        file.clone()
    } else {
        file.chars().skip(count - 57).collect()
    };
    println!("  {} 文件: {}", "✦".cyan(), file_display);
    println!(
        "  {} 模型: {} / {}",
        "✦".cyan(),
        provider_label,
        client.model
    );
    println!();

    let path = Path::new(&file);
    let is_pdf = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"));

    let pdf_scan = if is_pdf {
        Some(
            scan::inspect_pdf(path, 20)
                .map_err(|e| AppError::TaskPanic(format!("PDF 探测失败: {}", e)))?,
        )
    } else {
        None
    };

    if let Some(scan) = &pdf_scan {
        println!(
            "输入探测: {}，{}页，密度 {:.1} 字/页，置信度 {}%",
            match scan.status {
                PdfStatus::HasText => "已有文字层".green(),
                PdfStatus::NeedOcr => "需 OCR".yellow(),
                PdfStatus::HandDrawn => "手绘/矢量".yellow(),
                PdfStatus::Error => "异常".red(),
            },
            scan.pages,
            scan.text_density,
            scan.confidence
        );

        if scan.requires_ocr && !cli.force {
            let report_dir = cardnote_compiler::output::save_input_quality_report(
                &cli.output,
                &file,
                Some(scan),
                &quality::analyze(&format!("# PDF 需要 OCR\n\n{}", scan.reason)),
            )
            .await?;
            println!(
                "\n{} 输入需要 OCR，已中止以避免低质量内容进入 LLM。",
                "✗".red()
            );
            println!("质量报告已保存到: {}", report_dir);
            println!("如确认仍要继续，请使用 --force。");
            return Ok(());
        }
    }

    println!("读取文件...");
    let document = convert_to_markdown_async(&file).await?;
    println!("  {} {} 字符", "✓".green(), document.chars().count());

    let input_report = quality::analyze(&document);
    println!(
        "输入质量: {} ({:.1}/100)",
        input_report.grade().bold(),
        input_report.overall_score()
    );
    let report_dir = cardnote_compiler::output::save_input_quality_report(
        &cli.output,
        &file,
        pdf_scan.as_ref(),
        &input_report,
    )
    .await?;
    println!("输入质量报告已保存到: {}", report_dir);

    if !input_report.is_acceptable() && !cli.force {
        println!(
            "\n{} 输入质量为 {}，默认中止编译。",
            "✗".red(),
            input_report.grade()
        );
        println!("建议先 OCR、换源或清洗文本；如确认仍要继续，请使用 --force。");
        return Ok(());
    }

    let book_title = resolve_book_title(&file, is_pdf);
    // 自动注册到 .cardnote/books.json（如果还不存在）
    cardnote_compiler::config::ensure_book_registered(&book_title);

    // 创建带 timestamp 的输出目录，传给 Pipeline 用于实时写入每块结果
    let client_for_usage = client.clone();
    let timestamp = Local::now().format("%Y%m%d%H%M%S").to_string();
    let file_stem = Path::new(&file)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let file_stem = cardnote_compiler::output::sanitize_filename(&file_stem);
    let out_dir_name = format!("{}_{}", timestamp, file_stem);
    let output_dir = std::path::PathBuf::from(&cli.output).join(&out_dir_name);
    std::fs::create_dir_all(&output_dir)?;
    let pipeline = Pipeline::new(client, Some(output_dir.clone()));

    let compile_start = std::time::Instant::now();
    let result = pipeline.run(&document, &file, &book_title).await?;
    let compile_duration_ms = compile_start.elapsed().as_millis() as u64;

    // [C3] 编译结果健康检查：如果卡片为空，提示用户可能存在失败
    if result.cards.is_empty() && result.graph.entities.is_empty() {
        println!("\n{} 编译结果为空（实体和卡片均为空）。", "⚠".yellow());
        if !result.diagnostics.failures.is_empty() {
            println!(
                "   检测到 {} 个阶段失败，请检查 compile_diagnostics.md 了解详情。",
                result.diagnostics.failures.len()
            );
        } else {
            println!("   可能原因：LLM 输出全部为空，或卡片解析未匹配到任何有效内容。");
        }
        println!("   输出目录仍会创建，但文件内容可能为空。");
    }

    let doc_dir = Path::new("./documents");
    if let Err(e) = tokio::fs::create_dir_all(&doc_dir).await {
        eprintln!("  ⚠ 文档备份目录创建失败: {}", e);
    }
    if let Some(file_name) = Path::new(&file).file_name() {
        let timestamp = Local::now().format("%Y%m%d%H%M%S").to_string();
        let doc_name = format!("{}_{}", timestamp, file_name.to_string_lossy());
        let dest = doc_dir.join(&doc_name);
        if let Err(e) = tokio::fs::copy(&file, &dest).await {
            eprintln!("  ⚠ 文档备份复制失败 ({} -> {}): {}", file, dest.display(), e);
        }
    }

    let out_dir_str = output_dir.to_string_lossy().to_string();
    let output_path = if !result.chunks.is_empty() && result.chunks.len() > 1 {
        cardnote_compiler::output::save_book(
            &result.cards,
            &result.graph.entities,
            &out_dir_str,
            &book_title,
        )
        .await?
    } else {
        cardnote_compiler::output::save_single(&result, &out_dir_str).await?
    };
    println!("\n结果已保存到: {}", output_path);

    // 自动记录编译结果到 SQLite（事务包裹全部子表写入）
    if let Ok(tracker) = cardnote_compiler::batch::CompileTracker::new() {
        let (prompt, completion) = client_for_usage.usage_totals().await;
        let strategy = if document.chars().count() <= 200_000 {
            "extract_then_assign"
        } else {
            "map_reduce"
        };
        let (accepted, rejected) = result.cards.iter().fold((0, 0), |(a, r), c| {
            if c.status == cardnote_compiler::models::CardStatus::Accepted
                && c.reject_reason.is_empty()
            {
                (a + 1, r)
            } else {
                (a, r + 1)
            }
        });
        let pdf_meta = extract_pdf_metadata(&file);
        let file_size = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
        let file_format = Path::new(&file)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();
        let content_hash = cardnote_compiler::pipeline::fnv1a_hash_str(&document);

        let book_id = tracker
            .ensure_book(
                &file,
                &book_title,
                &pdf_meta.author,
                &pdf_meta.publisher,
                &pdf_meta.isbn,
                pdf_meta.page_count,
                file_size,
                &file_format,
                &content_hash,
            )
            .unwrap_or_else(|e| {
                eprintln!("  ⚠ 书籍注册失败 ({}): {} — book_id 置为 0", book_title, e);
                0
            });

        let markdown_path = std::path::Path::new(&output_path)
            .join("README.md")
            .to_string_lossy()
            .to_string();
        let error_message = if result.diagnostics.failures.is_empty() {
            String::new()
        } else {
            result
                .diagnostics
                .failures
                .iter()
                .map(|f| format!("{}: {}", f.stage, f.error))
                .collect::<Vec<_>>()
                .join("; ")
        };

        let cfg = cardnote_compiler::batch::RecordConfig {
            strategy: strategy.to_string(),
            model: client_for_usage.model.clone(),
            provider: provider.clone(),
            doc_chars: document.chars().count(),
            total_cards: result.cards.len(),
            accepted_cards: accepted,
            rejected_cards: rejected,
            entity_count: result.graph.entities.len(),
            relation_count: result.graph.relations.len(),
            prompt_tokens: prompt,
            completion_tokens: completion,
            output_dir: output_path.clone(),
            markdown_path,
            duration_ms: compile_duration_ms,
            success: !result.cards.is_empty(),
            error_message,
        };

        match tracker.record_compilation(book_id, &result, &cfg) {
            Ok(compilation_id) => {
                let success = !result.cards.is_empty();
                let _ = tracker.update_book_status(book_id, success);
                eprintln!("  ✓ 编译记录已持久化 (compilation_id={})", compilation_id);
            }
            Err(e) => {
                eprintln!("  ⚠ 编译记录持久化失败: {}", e);
            }
        }
    }

    let src_report = std::path::Path::new(&report_dir).join("input_quality_report.md");
    let dest_report = std::path::Path::new(&output_path).join("input_quality_report.md");
    if src_report.exists() {
        if let Err(e) = tokio::fs::copy(&src_report, &dest_report).await {
            eprintln!("  ⚠ 质量报告复制失败: {}", e);
        } else {
            let is_safe = match (
                std::fs::canonicalize(&report_dir),
                std::fs::canonicalize(&cli.output),
            ) {
                (Ok(r), Ok(o)) => r.starts_with(&o),
                _ => !std::path::Path::new(&report_dir)
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir)),
            };
            if is_safe {
                if let Err(e) = tokio::fs::remove_dir_all(&report_dir).await {
                    eprintln!("  ⚠ 临时报告目录删除失败: {}", e);
                }
            } else {
                eprintln!("  ⚠ 跳过删除异常路径: {}", report_dir);
            }
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════
// ═══════════════════════════════════════════════════════
//  History 命令处理器
// ═══════════════════════════════════════════════════════

async fn handle_history(limit: usize) -> cardnote_compiler::error::Result<()> {
    use cardnote_compiler::batch::CompileTracker;

    let tracker = CompileTracker::new()?;
    let stats = tracker.stats()?;
    let records = tracker.recent(limit)?;

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║               CardNote 编译历史                              ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!(
        "║  {} 本书 | {} 次编译 | {} 张卡片 | 待审阅 {} 本              ║",
        stats.unique_books, stats.total_compilations, stats.total_cards, stats.pending_review
    );
    println!(
        "║  累计 {} tokens                                             ║",
        stats.total_tokens
    );
    println!("╚══════════════════════════════════════════════════════════════╝");

    if records.is_empty() {
        println!("\n  暂无编译记录。运行 cardc <文件> 开始编译。");
        return Ok(());
    }

    println!();
    for r in &records {
        let review_mark = if r.reviewed { "✓" } else { "○" };
        println!(
            "  {} v{} {}  {} → {}/{} 张 (通过/拦截) | {}",
            review_mark,
            r.version,
            r.compiled_at,
            r.book_title,
            r.accepted_cards,
            r.rejected_cards,
            r.output_dir
        );
    }

    Ok(())
}

async fn handle_review(id: i64) -> cardnote_compiler::error::Result<()> {
    use cardnote_compiler::batch::CompileTracker;
    let tracker = CompileTracker::new()?;
    tracker.mark_reviewed(id)?;
    println!("  ✓ 编译记录 #{} 已标记为已审阅", id);
    Ok(())
}
