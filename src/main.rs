use std::path::Path;

use clap::{Parser, Subcommand};
use colored::*;

use cardnote_compiler::api::create_client;
use cardnote_compiler::config::{find_env_file, get_api_config, get_provider_label};
use cardnote_compiler::converter::{
    convert_to_markdown_async, convert_to_markdown_async_with_timeout, guess_title,
};
use cardnote_compiler::diagnostics;
use cardnote_compiler::models::Document;
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

    /// API Key
    #[arg(long)]
    api_key: Option<String>,

    /// 自定义 API 地址
    #[arg(long)]
    base_url: Option<String>,

    /// 低质量输入仍强制继续编译
    #[arg(long)]
    force: bool,

    /// 多文档策展模式标题
    #[arg(long)]
    book: Option<String>,
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

async fn run() -> anyhow::Result<()> {
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
            return Ok(());
        }
        Some(Commands::Doctor) => {
            diagnostics::cmd_doctor().await?;
            return Ok(());
        }
        Some(Commands::Quality { file }) => {
            println!("读取文件: {}", file);
            // quality 命令使用 60 秒短超时，避免扫描版 PDF 长时间挂起
            let document = match convert_to_markdown_async_with_timeout(&file, Some(60)).await {
                Ok(doc) => doc,
                Err(e) if e.to_string().contains("超时") => {
                    println!("  {} {}", "⚠".yellow(), e);
                    println!(
                        "\n  {} 该 PDF 可能是扫描版/图片版，OCR 处理耗时较长。",
                        "提示:".yellow().bold()
                    );
                    println!("  如需完整质量检测，请确保 OCR 工具可用，或使用文本版 PDF。\n");
                    // 生成一个简化的质量报告，标记为扫描版
                    let placeholder = format!(
                        "# 扫描版 PDF\n\n该文件 [{}] 被识别为扫描版/图片版 PDF。\n需要 OCR 工具提取文本内容。",
                        file
                    );
                    let report = quality::analyze(&placeholder);
                    quality::print_report(&report);
                    return Ok(());
                }
                Err(e) => return Err(e.into()),
            };
            println!("  {} {} 字符", "✓".green(), document.len());
            let report = quality::analyze(&document);
            quality::print_report(&report);
            return Ok(());
        }
        Some(Commands::Summary { file, output }) => {
            run_phase(
                "summary",
                &file,
                &output,
                cli.provider,
                cli.model,
                cli.api_key,
                cli.base_url,
            )
            .await?;
            return Ok(());
        }
        Some(Commands::Annotate { file }) => {
            run_phase(
                "annotate",
                &file,
                "./output",
                cli.provider,
                cli.model,
                cli.api_key,
                cli.base_url,
            )
            .await?;
            return Ok(());
        }
        Some(Commands::Cards { file, output }) => {
            run_phase(
                "cards",
                &file,
                &output,
                cli.provider,
                cli.model,
                cli.api_key,
                cli.base_url,
            )
            .await?;
            return Ok(());
        }
        Some(Commands::Graph { file }) => {
            run_phase(
                "graph",
                &file,
                "./output",
                cli.provider,
                cli.model,
                cli.api_key,
                cli.base_url,
            )
            .await?;
            return Ok(());
        }
        Some(Commands::Scan {
            dir,
            recursive,
            threshold,
        }) => {
            let report = scan::scan_directory_async(&dir, recursive, threshold)
                .await
                .map_err(|e| anyhow::anyhow!("扫描失败: {}", e))?;
            scan::print_scan_report(&report);
            return Ok(());
        }
        None => {}
    }

    // 主命令：完整编译
    let file = cli
        .file
        .ok_or_else(|| anyhow::anyhow!("请指定输入文件，或运行 cardc init / cardc doctor"))?;

    // 获取配置
    let (api_key, provider) = get_api_config(cli.api_key, cli.provider).await?;
    let client = create_client(&provider, &api_key, cli.model, cli.base_url)?;

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
    if let Some(ref book_title) = cli.book {
        println!("  {} 模式: 多文档策展「{}」", "✦".cyan(), book_title);
    }
    println!();

    let pipeline = Pipeline::new(client);

    if let Some(book_title) = cli.book {
        // 多文档策展
        let path = Path::new(&file);
        let md_files: Vec<_> = if path.is_dir() {
            walkdir::WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|ext| ext == "md").unwrap_or(false))
                .map(|e| e.path().to_path_buf())
                .collect()
        } else {
            vec![path.to_path_buf()]
        };

        if md_files.is_empty() {
            return Err(anyhow::anyhow!("未找到 Markdown 文件: {}", file));
        }

        println!("多文档策展模式: 发现 {} 篇文档", md_files.len());
        let mut documents = Vec::new();
        for f in md_files {
            let path_str = f.to_string_lossy();
            let content = convert_to_markdown_async(&path_str).await?;
            let title = guess_title(&content, &path_str);
            println!(
                "  {} {} ({} 字符)",
                "✓".green(),
                f.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "未知文件".to_string()),
                content.len()
            );
            documents.push(Document {
                title,
                content,
                source_file: f.to_string_lossy().to_string(),
            });
        }

        let output_path = pipeline
            .compile_book(documents, &book_title, &cli.output)
            .await?;
        println!("\n结果已保存到: {}", output_path);
    } else {
        // 单篇编译
        let path = Path::new(&file);
        let is_pdf = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"));

        // 如果是 PDF，优先尝试按章节拆分编译
        if is_pdf {
            match compile_by_chapters(&file, &pipeline, &cli.output).await {
                Ok(path) => {
                    println!("\n结果已保存到: {}", path);
                    return Ok(());
                }
                Err(e) => {
                    println!("  ℹ 按章节编译不可用: {}，回退到全书编译", e);
                }
            }
        }

        let pdf_scan = if is_pdf {
            Some(scan::inspect_pdf(path, 20).map_err(|e| anyhow::anyhow!("PDF 探测失败: {}", e))?)
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
        println!("  {} {} 字符", "✓".green(), document.len());

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

        if !result.chunks.is_empty() && result.chunks.len() > 1 {
            let output_path = cardnote_compiler::output::save_book(
                &result.summary,
                &result.cards,
                &result.graph.entities,
                &result.graph.relations,
                &cli.output,
                &result.summary.title,
            )
            .await?;
            println!("\n结果已保存到: {}", output_path);
            // 保存原始文件副本和 Markdown 文本
            save_source_docs(&file, &document, &output_path).await;
        } else {
            let output_path = cardnote_compiler::output::save_single(&result, &cli.output).await?;
            println!("\n结果已保存到: {}", output_path);
            // 保存原始文件副本和 Markdown 文本
            save_source_docs(&file, &document, &output_path).await;
        }
    }

    Ok(())
}

/// 保存原始文档副本和 Markdown 文本到输出目录
async fn save_source_docs(file_path: &str, document: &str, output_path: &str) {
    let output_dir = Path::new(output_path);

    // 保存原始文件副本
    if let Some(file_name) = Path::new(file_path).file_name() {
        let dest = output_dir.join(format!("source_{}", file_name.to_string_lossy()));
        tokio::fs::copy(file_path, &dest).await.ok();
    }

    // 保存转换后的 Markdown 文本
    let md_path = output_dir.join("_source_text.md");
    tokio::fs::write(&md_path, document).await.ok();
}

/// 按章节拆分编译 PDF
///
/// 1. 提取 PDF 目录结构
/// 2. 每章独立提取文本并编译
/// 3. 每章保存到独立子目录
/// 4. 汇总所有卡片到顶层目录
async fn compile_by_chapters(
    file_path: &str,
    pipeline: &Pipeline,
    output_dir: &str,
) -> anyhow::Result<String> {
    // 1. 获取章节列表
    let segments = cardnote_compiler::converter::split_pdf_by_toc(file_path)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    if segments.len() < 2 {
        return Err(anyhow::anyhow!(
            "PDF 目录章节数不足 ({} 章)，无法按章节拆分",
            segments.len()
        ));
    }

    // 2. 提取书名（PDF 文件名）
    let book_name = Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("未命名");

    println!("检测到 {} 个章节，按章节拆分编译...", segments.len());

    // 3. 创建基础输出目录
    let base_dir = cardnote_compiler::output::create_output_dir(output_dir, Some(book_name))
        .await
        .map_err(|e| anyhow::anyhow!("创建输出目录失败: {}", e))?;

    // 4. 临时目录用于拆分 PDF
    let temp_dir = tempfile::tempdir().map_err(|e| anyhow::anyhow!("创建临时目录失败: {}", e))?;

    let mut all_cards: Vec<cardnote_compiler::models::Card> = Vec::new();
    let mut success_count = 0;
    let mut failed_chapters: Vec<String> = Vec::new();

    for (idx, (title, start, end)) in segments.iter().enumerate() {
        println!("\n  [章节 {}/{}] {}", idx + 1, segments.len(), title);

        // 提取章节 PDF 到临时文件
        let seg_path = temp_dir.path().join(format!("seg_{:03}.pdf", idx));
        if let Err(e) = cardnote_compiler::converter::save_pdf_range(
            file_path,
            &seg_path.to_string_lossy(),
            *start,
            *end,
        ) {
            eprintln!("    ⚠ 章节拆分失败: {}", e);
            failed_chapters.push(title.clone());
            continue;
        }

        // 转换为 Markdown
        let chapter_md = match cardnote_compiler::converter::convert_to_markdown_async(
            &seg_path.to_string_lossy(),
        )
        .await
        {
            Ok(md) => md,
            Err(e) => {
                eprintln!("    ⚠ 章节文本提取失败: {}", e);
                failed_chapters.push(title.clone());
                continue;
            }
        };

        if chapter_md.trim().len() < 50 {
            println!("    ℹ 章节文本过短，跳过");
            continue;
        }

        // 编译章节
        let mut result = match pipeline.run(&chapter_md, file_path).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("    ⚠ 章节编译失败: {}", e);
                failed_chapters.push(title.clone());
                continue;
            }
        };

        // 补充精确的 ref 引用信息
        let precise_ref = format!(
            "《{}》| {} | 第{}-{}页",
            book_name,
            title,
            start + 1,
            end + 1
        );
        for card in &mut result.cards {
            if card.reference.is_empty() {
                card.reference = precise_ref.clone();
            } else {
                card.reference = format!("{} | {}", precise_ref, card.reference);
            }
        }

        // 保存章节结果到独立子目录
        let safe_title = cardnote_compiler::output::sanitize_filename(title);
        let chapter_dir = base_dir.join(&safe_title);
        tokio::fs::create_dir_all(&chapter_dir).await?;
        if let Err(e) = cardnote_compiler::output::save_single_to_dir(&result, &chapter_dir).await {
            eprintln!("    ⚠ 章节保存失败: {}", e);
            failed_chapters.push(title.clone());
            continue;
        }

        all_cards.extend(result.cards.clone());
        success_count += 1;
        println!("    ✓ {} 张卡片", result.cards.len());
    }

    // 保存汇总
    let summary = cardnote_compiler::models::Summary {
        title: book_name.to_string(),
        overview: format!(
            "共 {} 章 | 成功 {} 章 | 失败 {} 章 | {} 张卡片",
            segments.len(),
            success_count,
            failed_chapters.len(),
            all_cards.len()
        ),
        key_points: failed_chapters
            .iter()
            .map(|f| format!("⚠ 编译失败: {}", f))
            .collect(),
        structure: String::new(),
    };
    let summary_path = base_dir.join("summary.md");
    tokio::fs::write(&summary_path, summary.to_markdown()).await?;

    let all_cards_path = base_dir.join("all_cards.md");
    let content: Vec<String> = all_cards.iter().map(|c| c.to_markdown()).collect();
    tokio::fs::write(&all_cards_path, content.join("\n")).await?;

    // 保存原始文件副本到顶层目录
    if let Some(file_name) = Path::new(file_path).file_name() {
        let dest = base_dir.join(format!("source_{}", file_name.to_string_lossy()));
        tokio::fs::copy(file_path, &dest).await.ok();
    }

    println!(
        "\n  总计: {} 章成功 | {} 张卡片",
        success_count,
        all_cards.len()
    );
    if !failed_chapters.is_empty() {
        println!("  失败章节: {}", failed_chapters.join(", "));
    }

    Ok(base_dir.to_string_lossy().to_string())
}

async fn run_phase(
    phase: &str,
    file: &str,
    output_dir: &str,
    provider_arg: Option<String>,
    model_arg: Option<String>,
    api_key_arg: Option<String>,
    base_url_arg: Option<String>,
) -> anyhow::Result<()> {
    let (api_key, provider) = get_api_config(api_key_arg, provider_arg).await?;
    let client = create_client(&provider, &api_key, model_arg, base_url_arg)?;
    let pipeline = Pipeline::new(client);

    println!("读取文件: {}", file);
    let document = convert_to_markdown_async(file).await?;
    println!("  {} {} 字符", "✓".green(), document.len());

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
