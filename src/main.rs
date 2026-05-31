use std::path::Path;

use chrono::Local;
use clap::{Parser, Subcommand};
use colored::*;

use cardnote_compiler::api::create_client;
use cardnote_compiler::config::{find_env_file, get_api_config, get_provider_label};
use cardnote_compiler::health::{check_all_providers, print_health_report, select_best};
use cardnote_compiler::error::AppError;
use cardnote_compiler::converter::{
    convert_to_markdown_async, convert_to_markdown_async_with_timeout,
    extract_pdf_metadata,
};
use cardnote_compiler::diagnostics;
use cardnote_compiler::pipeline::Pipeline;
use cardnote_compiler::quality;
use cardnote_compiler::scan;
use cardnote_compiler::scan::PdfStatus;

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
    /// 仅运行 AI 摘要
    Summary {
        /// 输入文件
        file: String,
        #[arg(short, long, default_value = "./output")]
        output: String,
    },
    /// 仅运行 AI 标注
    Annotate {
        /// 输入文件
        file: String,
    },
    /// 仅运行 AI 卡片生成
    Cards {
        /// 输入文件
        file: String,
        #[arg(short, long, default_value = "./output")]
        output: String,
    },
    /// 仅运行 AI 图谱
    Graph {
        /// 输入文件
        file: String,
    },
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
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{} {}", "错误:".red().bold(), e);
        std::process::exit(1);
    }
}

// [H5] 阶段命令共享参数，避免上帝函数传参冗长
struct RunArgs {
    provider: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    base_url: Option<String>,
}

impl From<&Cli> for RunArgs {
    fn from(cli: &Cli) -> Self {
        Self {
            provider: cli.provider.clone(),
            model: cli.model.clone(),
            api_key: cli.api_key.clone(),
            base_url: cli.base_url.clone(),
        }
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
    let args: RunArgs = (&cli).into();

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
        Some(Commands::Summary { file, output }) => {
            handle_phase("summary", &file, &output, args).await
        }
        Some(Commands::Annotate { file }) => {
            handle_phase("annotate", &file, "./output", args).await
        }
        Some(Commands::Cards { file, output }) => {
            handle_phase("cards", &file, &output, args).await
        }
        Some(Commands::Graph { file }) => {
            handle_phase("graph", &file, "./output", args).await
        }
        Some(Commands::Scan {
            dir,
            recursive,
            threshold,
        }) => handle_scan(&dir, recursive, threshold).await,
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

// [H5] 主编译流程处理器（原 run() 中 None 分支的核心逻辑）
async fn handle_compile(cli: Cli) -> cardnote_compiler::error::Result<()> {
    let file = cli
        .file
        .ok_or_else(|| AppError::Config("请指定输入文件，或运行 cardc init / cardc doctor".to_string()))?;

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
            let cred = credentials.get(&best_provider_id)
                .ok_or_else(|| AppError::Config(format!("最佳 Provider {} 凭据丢失", best_provider_id)))?;
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
    let file_display = if file.chars().count() < 60 {
        file.clone()
    } else {
        file.chars()
            .rev()
            .take(57)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    };
    println!("  {} 文件: {}", "✦".cyan(), file_display);
    println!(
        "  {} 模型: {} / {}",
        "✦".cyan(),
        provider_label,
        client.model
    );
    println!();

    let pipeline = Pipeline::new(client);

    let path = Path::new(&file);
    let is_pdf = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"));

    let pdf_scan = if is_pdf {
        Some(scan::inspect_pdf(path, 20).map_err(|e| AppError::TaskPanic(format!("PDF 探测失败: {}", e)))?)
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

    let result = pipeline.run(&document, &file).await?;

    let book_title = if is_pdf {
        let meta = extract_pdf_metadata(&file);
        if !meta.title.is_empty() {
            meta.title
        } else {
            Path::new(&file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&result.summary.title)
                .to_string()
        }
    } else {
        Path::new(&file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&result.summary.title)
            .to_string()
    };

    let doc_dir = Path::new("./documents");
    tokio::fs::create_dir_all(&doc_dir).await.ok();
    if let Some(file_name) = Path::new(&file).file_name() {
        let timestamp = Local::now().format("%Y%m%d%H%M%S").to_string();
        let doc_name = format!("{}_{}", timestamp, file_name.to_string_lossy());
        let dest = doc_dir.join(&doc_name);
        tokio::fs::copy(&file, &dest).await.ok();
    }

    let output_path = if !result.chunks.is_empty() && result.chunks.len() > 1 {
        cardnote_compiler::output::save_book(
            &result.summary,
            &result.cards,
            &result.graph.entities,
            &result.graph.relations,
            &cli.output,
            &book_title,
        )
        .await?
    } else {
        cardnote_compiler::output::save_single(&result, &cli.output).await?
    };
    println!("\n结果已保存到: {}", output_path);

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
                _ => !report_dir.contains(".."),
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

// [H5] 阶段命令处理器（summary / annotate / cards / graph）
async fn handle_phase(
    phase: &str,
    file: &str,
    output_dir: &str,
    args: RunArgs,
) -> cardnote_compiler::error::Result<()> {
    let (api_key, provider) = get_api_config(args.api_key, args.provider).await?;
    let client = create_client(&provider, &api_key, args.model, args.base_url)?;
    let pipeline = Pipeline::new(client);

    println!("读取文件: {}", file);
    let document = convert_to_markdown_async(file).await?;
    // [H3] 使用 chars().count() 获取真实字符数（中文不再虚高 3 倍）
    println!("  {} {} 字符", "✓".green(), document.chars().count());

    match phase {
        "summary" => {
            let result = pipeline.run_summary(&document).await?;
            println!("\n{}", "=".repeat(60));
            println!("{}", result.to_markdown());
        }
        "annotate" => {
            let entities = pipeline.run_entities(&document).await?;
            for e in &entities {
                println!("  - {} ({})", e.name, e.entity_type);
            }
            println!("\n共识别 {} 个实体", entities.len());
        }
        "cards" => {
            let cards = pipeline.run_cards(&document).await?;
            cardnote_compiler::output::save_cards_by_type(Path::new(output_dir), &cards).await?;
            println!("\n共生成 {} 张卡片", cards.len());
            println!("已保存到: {}/", output_dir);
        }
        "graph" => {
            let entities = pipeline.run_entities(&document).await?;
            println!("  实体: {} 个", entities.len());
            let graph = pipeline.run_graph(&document, &entities).await?;
            println!();
            println!("{}", graph.to_mermaid());
        }
        _ => {}
    }

    println!("\n{} {} 完成", "✓".green(), phase);
    Ok(())
}
