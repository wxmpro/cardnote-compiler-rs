use std::path::{Path, PathBuf};
use std::process::Command;

use colored::*;

use crate::error::{AppError, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum PdfStatus {
    NeedOcr,
    HasText,
    HandDrawn,
    Error,
}

/// PDF质量评分（0-100）
#[derive(Debug, Clone)]
pub struct PdfQualityScore {
    /// 综合评分
    pub overall: u8,
    /// 文字层质量（0-100）
    pub text_layer: u8,
    /// 排版结构质量（0-100）
    pub structure: u8,
    /// 图像质量（0-100，仅扫描版）
    pub image_quality: u8,
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub path: String,
    pub status: PdfStatus,
    pub pages: usize,
    pub char_count: usize,
    pub reason: String,
    pub text_density: f32,
    pub requires_ocr: bool,
    pub confidence: u8,
    pub detection_method: String,
    pub sampled_pages: usize,
    /// PDF质量评分
    pub quality: PdfQualityScore,
}

pub struct ScanReport {
    pub directory: String,
    pub total: usize,
    pub need_ocr: Vec<ScanResult>,
    pub has_text: Vec<ScanResult>,
    pub hand_drawn: Vec<ScanResult>,
    pub errors: Vec<ScanResult>,
}

pub fn scan_directory(dir: &str, recursive: bool, threshold: usize) -> Result<ScanReport> {
    let pdf_files = find_pdf_files(dir, recursive)?;

    if pdf_files.is_empty() {
        return Ok(ScanReport {
            directory: dir.to_string(),
            total: 0,
            need_ocr: vec![],
            has_text: vec![],
            hand_drawn: vec![],
            errors: vec![],
        });
    }

    println!(
        "共找到 {} 个 PDF 文件，开始检测文字层...",
        pdf_files.len().to_string().bright_white()
    );
    println!("判定阈值: {} 个字符", threshold.to_string().bright_black());
    println!("{}", "-".repeat(60).bright_black());

    let mut results = Vec::new();
    for (i, path) in pdf_files.iter().enumerate() {
        let result = detect_pdf_status(path, threshold)?;

        let label = match result.status {
            PdfStatus::NeedOcr => "需 OCR".yellow(),
            PdfStatus::HasText => "已有文字层".green(),
            PdfStatus::HandDrawn => "手绘版".cyan(),
            PdfStatus::Error => "异常".red(),
        };
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        println!(
            "[{:>3}/{}] {:<10} | {:<40} | {}",
            i + 1,
            pdf_files.len(),
            label,
            name,
            result.reason.bright_black()
        );

        results.push(result);
    }

    let need_ocr: Vec<_> = results
        .iter()
        .filter(|r| r.status == PdfStatus::NeedOcr)
        .cloned()
        .collect();
    let has_text: Vec<_> = results
        .iter()
        .filter(|r| r.status == PdfStatus::HasText)
        .cloned()
        .collect();
    let hand_drawn: Vec<_> = results
        .iter()
        .filter(|r| r.status == PdfStatus::HandDrawn)
        .cloned()
        .collect();
    let errors: Vec<_> = results
        .iter()
        .filter(|r| r.status == PdfStatus::Error)
        .cloned()
        .collect();

    Ok(ScanReport {
        directory: dir.to_string(),
        total: results.len(),
        need_ocr,
        has_text,
        hand_drawn,
        errors,
    })
}

/// 异步并行版本：使用 tokio::task::spawn_blocking 并发检测多个 PDF
pub async fn scan_directory_async(
    dir: &str,
    recursive: bool,
    threshold: usize,
) -> Result<ScanReport> {
    let pdf_files = find_pdf_files(dir, recursive)?;

    if pdf_files.is_empty() {
        return Ok(ScanReport {
            directory: dir.to_string(),
            total: 0,
            need_ocr: vec![],
            has_text: vec![],
            hand_drawn: vec![],
            errors: vec![],
        });
    }

    println!(
        "共找到 {} 个 PDF 文件，开始并行检测文字层...",
        pdf_files.len().to_string().bright_white()
    );
    println!("判定阈值: {} 个字符", threshold.to_string().bright_black());
    println!("{}", "-".repeat(60).bright_black());

    // 并行检测：每个文件一个 spawn_blocking 任务
    let mut handles = Vec::new();
    for (idx, path) in pdf_files.into_iter().enumerate() {
        let handle = tokio::task::spawn_blocking(move || {
            let result = detect_pdf_status(&path, threshold)?;
            Ok::<(usize, ScanResult), AppError>((idx, result))
        });
        handles.push(handle);
    }

    // 收集结果
    let mut indexed_results: Vec<(usize, ScanResult)> = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(Ok((idx, result))) => indexed_results.push((idx, result)),
            Ok(Err(e)) => {
                eprintln!("  {} 检测失败: {}", "✗".red(), e);
            }
            Err(e) => {
                eprintln!("  {} 任务 panic: {}", "✗".red(), e);
            }
        }
    }

    // 按原始顺序排序
    indexed_results.sort_by_key(|(idx, _)| *idx);
    let results: Vec<ScanResult> = indexed_results.into_iter().map(|(_, r)| r).collect();

    // 统一打印结果（保持顺序）
    let total = results.len();
    for (i, result) in results.iter().enumerate() {
        let label = match result.status {
            PdfStatus::NeedOcr => "需 OCR".yellow(),
            PdfStatus::HasText => "已有文字层".green(),
            PdfStatus::HandDrawn => "手绘版".cyan(),
            PdfStatus::Error => "异常".red(),
        };
        let name = Path::new(&result.path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        println!(
            "[{:>3}/{}] {:<10} | {:<40} | {}",
            i + 1,
            total,
            label,
            name,
            result.reason.bright_black()
        );
    }

    let need_ocr: Vec<_> = results
        .iter()
        .filter(|r| r.status == PdfStatus::NeedOcr)
        .cloned()
        .collect();
    let has_text: Vec<_> = results
        .iter()
        .filter(|r| r.status == PdfStatus::HasText)
        .cloned()
        .collect();
    let hand_drawn: Vec<_> = results
        .iter()
        .filter(|r| r.status == PdfStatus::HandDrawn)
        .cloned()
        .collect();
    let errors: Vec<_> = results
        .iter()
        .filter(|r| r.status == PdfStatus::Error)
        .cloned()
        .collect();

    Ok(ScanReport {
        directory: dir.to_string(),
        total: results.len(),
        need_ocr,
        has_text,
        hand_drawn,
        errors,
    })
}

fn find_pdf_files(dir: &str, recursive: bool) -> Result<Vec<PathBuf>> {
    let path = Path::new(dir);
    if !path.exists() {
        return Err(AppError::FileNotFound(dir.to_string()));
    }

    let mut files = Vec::new();

    if recursive {
        for entry in walkdir::WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            if let Some(ext) = entry.path().extension()
                && ext.to_string_lossy().to_lowercase() == "pdf"
            {
                files.push(entry.path().to_path_buf());
            }
        }
    } else if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            if let Some(ext) = entry.path().extension()
                && ext.to_string_lossy().to_lowercase() == "pdf"
            {
                files.push(entry.path());
            }
        }
    }

    files.sort();
    Ok(files)
}

/// 查找 PDF Expert OCR 项目路径
pub fn find_ocr_project() -> Option<PathBuf> {
    // 1. 检查 $CARDNOTE_OCR_PROJECT_PATH 环境变量
    if let Ok(path) = std::env::var("CARDNOTE_OCR_PROJECT_PATH") {
        let p = PathBuf::from(&path);
        if p.exists() {
            return Some(p);
        }
    }

    // 2. 相对路径：从当前项目上一级查找 pdf-expert-batch-ocr
    if let Ok(cwd) = std::env::current_dir() {
        let relative = cwd.parent().map(|p| p.join("pdf-expert-batch-ocr"));
        if let Some(p) = relative {
            if p.exists() {
                return Some(p);
            }
        }
    }

    // 3. 在 HOME 下尝试常见位置
    if let Ok(home) = std::env::var("HOME") {
        let candidates = [
            PathBuf::from(&home).join("cardnote-projects/pdf-expert-batch-ocr"),
            PathBuf::from(&home).join("projects/pdf-expert-batch-ocr"),
            PathBuf::from(&home).join("code/pdf-expert-batch-ocr"),
        ];
        for p in &candidates {
            if p.exists() {
                return Some(p.clone());
            }
        }
    }

    None
}

fn scan_confidence(
    status: &PdfStatus,
    pages: usize,
    char_count: usize,
    sampled_pages: usize,
) -> u8 {
    match status {
        PdfStatus::HasText => {
            if char_count >= sampled_pages.max(1) * 100 {
                95
            } else {
                80
            }
        }
        PdfStatus::NeedOcr => {
            if pages == 0 || char_count == 0 {
                90
            } else {
                75
            }
        }
        PdfStatus::HandDrawn => 70,
        PdfStatus::Error => 0,
    }
}

fn make_scan_result(
    path: &Path,
    status: PdfStatus,
    pages: usize,
    char_count: usize,
    reason: String,
    sampled_pages: usize,
    detection_method: &str,
    font_count: usize,
    image_count: usize,
) -> ScanResult {
    let effective_sample = sampled_pages.max(1);
    let text_density = char_count as f32 / effective_sample as f32;
    let requires_ocr = matches!(status, PdfStatus::NeedOcr | PdfStatus::HandDrawn);
    let confidence = scan_confidence(&status, pages, char_count, sampled_pages);
    let quality = compute_pdf_quality(&status, pages, font_count, image_count, sampled_pages);

    ScanResult {
        path: path.to_string_lossy().to_string(),
        status,
        pages,
        char_count,
        reason,
        text_density,
        requires_ocr,
        confidence,
        detection_method: detection_method.to_string(),
        sampled_pages,
        quality,
    }
}

pub fn inspect_pdf(pdf_path: &Path, threshold: usize) -> Result<ScanResult> {
    detect_pdf_status(pdf_path, threshold)
}

/// 纯 Rust 版本：用 lopdf 检测 PDF 类型，无需启动 Python 子进程
fn detect_pdf_status(pdf_path: &Path, _threshold: usize) -> Result<ScanResult> {
    use lopdf::{Document, Object};

    let doc = match Document::load(pdf_path) {
        Ok(d) => d,
        Err(_) => {
            // lopdf 解析失败时 fallback 到 PyMuPDF（对损坏/非标准 PDF 更宽容）
            return detect_pdf_status_python(pdf_path, _threshold);
        }
    };

    let pages = doc.get_pages();
    let page_count = pages.len();

    if page_count == 0 {
        return Ok(make_scan_result(
            pdf_path,
            PdfStatus::NeedOcr,
            0,
            0,
            "0 页，判定为扫描版".to_string(),
            0,
            "lopdf",
            0,
            0,
        ));
    }

    // 抽样检测：最多检查前 5 页
    let sample_size = page_count.min(5);
    let page_ids: Vec<_> = pages.values().take(sample_size).copied().collect();

    let mut total_fonts = 0usize;
    let mut total_images = 0usize;

    for page_id in &page_ids {
        if let Ok(page_obj) = doc.get_object(*page_id)
            && let Ok(dict) = page_obj.as_dict()
        {
            // 检查 Resources -> Font
            if let Ok(resources) = dict.get(b"Resources") {
                let res_dict = match resources {
                    Object::Reference(id) => {
                        if let Ok(res_obj) = doc.get_object(*id) {
                            res_obj.as_dict().ok()
                        } else {
                            None
                        }
                    }
                    Object::Dictionary(d) => Some(d),
                    _ => None,
                };
                if let Some(res_dict) = res_dict {
                    if let Ok(font_obj) = res_dict.get(b"Font") {
                        match font_obj {
                            Object::Dictionary(d) => total_fonts += d.len(),
                            Object::Reference(id) => {
                                if let Ok(obj) = doc.get_object(*id)
                                    && let Ok(d) = obj.as_dict()
                                {
                                    total_fonts += d.len();
                                }
                            }
                            _ => {}
                        }
                    }
                    // 检查 Resources -> XObject -> Image
                    if let Ok(xobj_obj) = res_dict.get(b"XObject") {
                        match xobj_obj {
                            Object::Dictionary(d) => {
                                for (_, v) in d.iter() {
                                    if let Object::Reference(id) = v
                                        && let Ok(obj) = doc.get_object(*id)
                                        && let Ok(xd) = obj.as_dict()
                                        && let Ok(subtype) = xd.get(b"Subtype")
                                        && let Ok(name) = subtype.as_name()
                                        && name == b"Image"
                                    {
                                        total_images += 1;
                                    }
                                }
                            }
                            Object::Reference(id) => {
                                if let Ok(obj) = doc.get_object(*id)
                                    && let Ok(d) = obj.as_dict()
                                {
                                    for (_, v) in d.iter() {
                                        if let Object::Reference(id2) = v
                                            && let Ok(obj2) = doc.get_object(*id2)
                                            && let Ok(xd) = obj2.as_dict()
                                            && let Ok(subtype) = xd.get(b"Subtype")
                                            && let Ok(name) = subtype.as_name()
                                            && name == b"Image"
                                        {
                                            total_images += 1;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // 手绘版检测：极少字体 + 极少图片 + 大量间接对象（矢量路径的代理指标）
    let has_drawings = total_fonts == 0 && total_images == 0 && doc.objects.len() > page_count * 20;

    // 判断逻辑
    // 有字体定义 => 大概率有文字层（扫描版 PDF 通常没有字体）
    let (status, reason) = if total_fonts > 0 {
        (
            PdfStatus::HasText,
            format!("检测到 {} 个字体定义", total_fonts),
        )
    } else if has_drawings {
        (
            PdfStatus::HandDrawn,
            format!("无字体, 对象数 {}, 判定为手绘版", doc.objects.len()),
        )
    } else {
        (PdfStatus::NeedOcr, "无字体定义, 判定为扫描版".to_string())
    };

    Ok(make_scan_result(
        pdf_path,
        status,
        page_count,
        total_fonts,
        reason,
        sample_size,
        "lopdf",
        total_fonts,
        total_images,
    ))
}

pub fn print_scan_report(report: &ScanReport) {
    println!(
        "\n{}",
        "╔════════════════════════════════════════════════════════════╗".bright_cyan()
    );
    println!(
        "{}",
        "║              PDF 预扫描检测报告                            ║".bright_cyan()
    );
    println!(
        "{}",
        "╚════════════════════════════════════════════════════════════╝".bright_cyan()
    );

    println!("\n  扫描目录: {}", report.directory.bright_white());
    println!("  总计文件: {}", report.total.to_string().bright_white());

    // 计算平均质量评分
    let all_results: Vec<_> = report
        .has_text
        .iter()
        .chain(report.need_ocr.iter())
        .chain(report.hand_drawn.iter())
        .collect();
    let avg_quality = if !all_results.is_empty() {
        all_results
            .iter()
            .map(|r| r.quality.overall as u32)
            .sum::<u32>()
            / all_results.len() as u32
    } else {
        0
    };

    println!("\n  {}", "分类统计".bold());
    println!(
        "     {} 已有文字层: {} ({:.1}%)",
        "✓".green(),
        report.has_text.len().to_string().green(),
        pct(report.has_text.len(), report.total)
    );
    println!(
        "     {} 需 OCR:     {} ({:.1}%)",
        "⚠".yellow(),
        report.need_ocr.len().to_string().yellow(),
        pct(report.need_ocr.len(), report.total)
    );
    if !report.hand_drawn.is_empty() {
        println!(
            "     {} 手绘版:     {} ({:.1}%)",
            "◎".cyan(),
            report.hand_drawn.len().to_string().cyan(),
            pct(report.hand_drawn.len(), report.total)
        );
    }
    if !report.errors.is_empty() {
        println!(
            "     {} 异常:       {} ({:.1}%)",
            "✗".red(),
            report.errors.len().to_string().red(),
            pct(report.errors.len(), report.total)
        );
    }

    println!("\n  {}", "质量评分".bold());
    println!(
        "     综合评分: {} ({})",
        format!("{}/100", avg_quality).color(quality_color(avg_quality as u8)),
        quality_label(avg_quality as u8).color(quality_color(avg_quality as u8))
    );
    if !all_results.is_empty() {
        println!(
            "\n     类型  {:<40} {:>6} {:>8} {:>8} {:>8}",
            "文件名", "综合", "文字层", "排版", "图像"
        );
        println!("     {}", "─".repeat(75).bright_black());
        for r in all_results.iter().take(10) {
            let name = Path::new(&r.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?");
            let label = match r.status {
                PdfStatus::HasText => "文字",
                PdfStatus::NeedOcr => "扫描",
                PdfStatus::HandDrawn => "手绘",
                PdfStatus::Error => "错误",
            };
            println!(
                "     {}  {:<40} {:>6} {:>8} {:>8} {:>8}",
                label,
                if name.len() > 38 { &name[..38] } else { name },
                format!("{}", r.quality.overall).color(quality_color(r.quality.overall)),
                r.quality.text_layer,
                r.quality.structure,
                r.quality.image_quality,
            );
        }
        if all_results.len() > 10 {
            println!("     ... 还有 {} 个文件", all_results.len() - 10);
        }
    }

    println!("\n  {}", "下一步建议".bold());

    if report.need_ocr.is_empty() {
        println!("     所有 PDF 均可直接编译: {}", "cardc <文件>".green());
    } else {
        print_ocr_guidance(report);
    }

    println!();
}

fn pct(part: usize, total: usize) -> f32 {
    if total == 0 {
        0.0
    } else {
        part as f32 / total as f32 * 100.0
    }
}

fn print_ocr_guidance(report: &ScanReport) {
    let is_macos = cfg!(target_os = "macos");
    let ocr_project = find_ocr_project();
    let has_ocr_project = ocr_project.is_some();

    if is_macos && has_ocr_project {
        let project_path = ocr_project.unwrap();
        println!("     检测到 macOS + OCR 项目可用，建议批量 OCR:");
        println!(
            "       {} cd {}",
            "❯".cyan(),
            project_path.display().to_string().cyan()
        );
        println!(
            "       {} python scan_pdfs.py {} --output ocr_queue.json",
            "❯".cyan(),
            report.directory
        );
        println!(
            "       {} python batch_ocr.py --queue ocr_queue.json --output-dir ./output",
            "❯".cyan()
        );
    } else if is_macos {
        println!("     macOS 用户建议:");
        println!("       方案1: PDF Expert 手动 OCR（质量最好）");
        println!(
            "       方案2: 安装 MinerU: {}",
            "uv pip install -U 'mineru[core]'".cyan()
        );
    } else {
        println!("     非 macOS 用户建议:");
        println!(
            "       方案1: 安装 MinerU: {}",
            "uv pip install -U 'mineru[core]'".cyan()
        );
        println!("       方案2: 使用 Docker 运行 PaddleOCR");
    }

    if !report.has_text.is_empty() {
        println!("\n     以下文件可直接编译:");
        for r in &report.has_text {
            let name = Path::new(&r.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?");
            println!("       {} {}", "✓".green(), name.green());
        }
    }
}

/// 生成扫描版 PDF 的 OCR 指引提示（用于转换失败时）
pub fn ocr_guidance_for_file(file_path: &str) -> String {
    let is_macos = cfg!(target_os = "macos");
    let ocr_project = find_ocr_project();
    let has_ocr_project = ocr_project.is_some();
    let file_name = Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(file_path);

    let mut msg = format!(
        "\n\n  💡 PDF '{}' 为扫描版/图片版，需要 OCR 处理\n",
        file_name
    );

    if is_macos && has_ocr_project {
        let project_path = ocr_project.unwrap();
        msg.push_str(&format!(
            "\n     OCR 流水线项目可用，运行以下命令:\n\
             \x20      cd {}\n\
             \x20      python scan_pdfs.py {} --output ocr_queue.json\n\
             \x20      python batch_ocr.py --queue ocr_queue.json --output-dir ./output\n",
            project_path.display(),
            Path::new(file_path)
                .parent()
                .unwrap_or(Path::new("."))
                .display()
        ));
    } else if is_macos {
        msg.push_str("\n     macOS 用户建议:\n");
        msg.push_str("     1. PDF Expert 手动 OCR（质量最好）\n");
        msg.push_str("     2. 安装 MinerU: uv pip install -U 'mineru[core]'\n");
    } else {
        msg.push_str("\n     非 macOS 用户建议:\n");
        msg.push_str("     1. 安装 MinerU: uv pip install -U 'mineru[core]'\n");
        msg.push_str("     2. 使用 Docker 运行 PaddleOCR\n");
    }

    msg
}

/// 检查 OCR 流水线项目是否可用
pub fn ocr_pipeline_available() -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    if let Some(project_path) = find_ocr_project() {
        project_path.join("scan_pdfs.py").exists()
    } else {
        false
    }
}

/// PyMuPDF fallback：当 lopdf 解析失败时使用（对损坏/非标准 PDF 更宽容）
/// 计算PDF质量评分
fn compute_pdf_quality(
    status: &PdfStatus,
    page_count: usize,
    font_count: usize,
    image_count: usize,
    _sample_pages: usize,
) -> PdfQualityScore {
    match status {
        PdfStatus::HasText => {
            // 有文字层的PDF：文字层质量 = 字体丰富度
            let text_layer = if font_count >= 3 {
                95
            } else if font_count >= 2 {
                85
            } else if font_count >= 1 {
                75
            } else {
                60
            };

            // 结构质量：基于字体数量和页面数推断
            let structure = if font_count >= 2 && page_count > 1 {
                90
            } else if font_count >= 1 {
                80
            } else {
                65
            };

            // 图像质量：无图像为最佳（纯文字PDF）
            let image_quality = if image_count == 0 {
                95
            } else if image_count <= 2 {
                85
            } else {
                70
            };

            let overall = ((text_layer as u16 + structure as u16 + image_quality as u16) / 3) as u8;

            PdfQualityScore {
                overall,
                text_layer,
                structure,
                image_quality,
            }
        }
        PdfStatus::NeedOcr => {
            // 扫描版PDF：质量取决于图像复杂度
            let text_layer = 20; // 无文字层
            let structure = 30; // 无法直接判断结构

            // 图像质量：图像越少，OCR越容易
            let image_quality = if image_count == 0 {
                60 // 纯扫描页，可能分辨率尚可
            } else if image_count <= 2 {
                50
            } else {
                35 // 图像多，干扰多
            };

            let overall = ((text_layer as u16 + structure as u16 + image_quality as u16) / 3) as u8;

            PdfQualityScore {
                overall,
                text_layer,
                structure,
                image_quality,
            }
        }
        PdfStatus::HandDrawn => PdfQualityScore {
            overall: 30,
            text_layer: 15,
            structure: 25,
            image_quality: 50,
        },
        PdfStatus::Error => PdfQualityScore {
            overall: 0,
            text_layer: 0,
            structure: 0,
            image_quality: 0,
        },
    }
}

/// 质量评分颜色
fn quality_color(score: u8) -> colored::Color {
    match score {
        80..=100 => colored::Color::Green,
        60..=79 => colored::Color::Yellow,
        40..=59 => colored::Color::BrightYellow,
        _ => colored::Color::Red,
    }
}

/// 质量等级文字
fn quality_label(score: u8) -> &'static str {
    match score {
        80..=100 => "优",
        60..=79 => "良",
        40..=59 => "中",
        _ => "差",
    }
}

fn detect_pdf_status_python(pdf_path: &Path, threshold: usize) -> Result<ScanResult> {
    let output = Command::new("python3")
        .arg("-c")
        .arg(
            r#"
import sys, json
try:
    import pymupdf
    doc = pymupdf.open(sys.argv[1])
    total_chars = 0
    page_count = len(doc)
    for page in doc:
        text = page.get_text()
        total_chars += len(text.strip())
        if total_chars > int(sys.argv[2]):
            break
    total_drawings = 0
    total_images = 0
    for page in doc:
        try:
            total_drawings += len(page.get_drawings())
        except:
            pass
        try:
            total_images += len(page.get_images())
        except:
            pass
    doc.close()
    if total_chars > int(sys.argv[2]):
        status = "has_text"
        reason = f"已提取 {total_chars} 个字符"
    elif total_drawings > 50 and total_images < 3:
        status = "hand_drawn"
        reason = f"仅 {total_chars} 字符，但检测到大量矢量路径"
    else:
        status = "need_ocr"
        reason = f"仅 {total_chars} 字符，判定为扫描版"
    print(json.dumps({"status": status, "pages": page_count,
        "char_count": total_chars, "reason": reason}))
except Exception as e:
    print(json.dumps({"status": "error", "pages": 0,
        "char_count": 0, "reason": str(e)}))
"#,
        )
        .arg(pdf_path)
        .arg(threshold.to_string())
        .output()
        .map_err(|e| AppError::Conversion(format!("PDF 检测失败: {}", e)))?;

    let json_str = String::from_utf8_lossy(&output.stdout);
    let data: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| AppError::JsonParse(format!("检测输出解析失败: {}", e)))?;

    let status = match data["status"].as_str() {
        Some("need_ocr") => PdfStatus::NeedOcr,
        Some("has_text") => PdfStatus::HasText,
        Some("hand_drawn") => PdfStatus::HandDrawn,
        _ => PdfStatus::Error,
    };

    let pages = data["pages"].as_u64().unwrap_or(0) as usize;
    let char_count = data["char_count"].as_u64().unwrap_or(0) as usize;
    Ok(make_scan_result(
        pdf_path,
        status,
        pages,
        char_count,
        data["reason"].as_str().unwrap_or("").to_string(),
        pages.min(5),
        "pymupdf",
        0,
        0,
    ))
}
