use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;

use regex::Regex;

// 标题检测正则已提升为共享：crate::pipeline::heading_regex()

/// 预编译正则：匹配一级标题
static H1_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#\s+(.+)$").expect("硬编码正则应始终有效"));

/// 预编译正则：匹配二级标题
static H2_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^##\s+(.+)$").expect("硬编码正则应始终有效"));

/// 预编译正则：匹配标题行前缀
static HEADING_PREFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#\s+").expect("硬编码正则应始终有效"));

/// 预编译正则：匹配"第 X 页/章/节/篇"
static PAGE_CHAPTER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"第\s*\d+\s*[页章节篇]").expect("硬编码正则应始终有效"));

/// 预编译正则：匹配英文 Page/Chapter/Section/Part + 数字
static ENG_MARKER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(Page|Chapter|Section|Part)\s*\d+").expect("硬编码正则应始终有效")
});

use crate::config::{
    MAX_FILE_SIZE_MB, MAX_TEXT_FILE_STREAM_MB, PDF_CONVERT_TIMEOUT, PDF_FALLBACK_MIN_TEXT,
    PDF_LARGE_FILE_MB, PDF_SPLIT_PAGES_DEFAULT, PDF_TOC_MIN_ENTRIES, is_other_format,
    is_pdf_format, is_text_format,
};
use crate::error::{AppError, Result};
use crate::scan::{PdfStatus, find_ocr_project, inspect_pdf};

// 注：convert_to_markdown_async_with_timeout 已提供外层 timeout 兜底

/// 文本/PDF转换超时常量(秒)
#[allow(dead_code)]
const CONVERT_TIMEOUT_SECS: u64 = 300;
/// 扫描版PDF OCR超时常量(秒) — OCR需要更长时间
const OCR_TIMEOUT_SECS: u64 = 1800;

/// 检查文件大小，超过上限返回错误
fn check_file_size(path: &Path) -> Result<u64> {
    let metadata = std::fs::metadata(path)?;
    let size_mb = metadata.len() / 1024 / 1024;
    if size_mb > MAX_FILE_SIZE_MB {
        return Err(AppError::FileTooLarge(format!(
            "文件大小 {}MB 超过上限 {}MB",
            size_mb, MAX_FILE_SIZE_MB
        )));
    }
    Ok(size_mb)
}

/// 将任意输入文件转换为 Markdown 文本（同步）
pub fn convert_to_markdown(file_path: &str) -> Result<String> {
    // 路径安全验证：禁止 null 字节，禁止路径遍历
    if file_path.contains('\0') || file_path.contains("..") {
        return Err(AppError::FileNotFound(
            "路径包含非法字符或路径遍历尝试".to_string(),
        ));
    }
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(AppError::FileNotFound(file_path.to_string()));
    }

    // 规范化路径：消除符号链接和相对路径，确保是绝对路径
    let canonical = std::fs::canonicalize(path)
        .map_err(|e| AppError::FileNotFound(format!("路径规范化失败: {}", e)))?;
    let file_path = canonical
        .to_str()
        .ok_or_else(|| AppError::FileNotFound("路径包含非法字符".to_string()))?;

    // OOM 防护：检查文件大小（使用规范化后的路径）
    let _file_mb = check_file_size(&canonical)?;

    let suffix = format!(
        ".{}",
        canonical.extension().and_then(|e| e.to_str()).unwrap_or("")
    )
    .to_lowercase();

    if is_text_format(&suffix) {
        return read_text(file_path);
    }

    if is_pdf_format(&suffix) {
        return read_pdf(file_path);
    }

    if is_other_format(&suffix) {
        return read_markitdown(file_path);
    }

    // 兜底：尝试当作文本读取
    read_text(file_path)
}

/// 计算文本解码质量分，用于区分模糊编码
///
/// 评分规则：
/// - CJK 统一表意文字（中文）: +10
/// - 韩文音节（Hangul）: +10
/// - 日文平假名/片假名: +5
/// - ASCII 字母/数字: +1
/// - 控制字符（除 0x09/0x0A/0x0D）: -5
/// - Unicode 替换字符 U+FFFD: -10
fn decode_quality_score(text: &str) -> i32 {
    let mut score = 0;
    for ch in text.chars() {
        let cp = ch as u32;
        match cp {
            0x4E00..=0x9FFF => score += 10,
            0xAC00..=0xD7A3 => score += 10,
            0x3040..=0x309F => score += 10,
            0x30A0..=0x30FF => score += 10,
            0xFFFD => score -= 10,
            0x00..=0x08 | 0x0B..=0x0C | 0x0E..=0x1F => score -= 5,
            _ => {
                if ch.is_ascii_alphanumeric() {
                    score += 1;
                }
            }
        }
    }
    score
}

/// 尝试用指定编码解码并返回（文本, 质量分）
fn try_decode(bytes: &[u8], encoding: &'static encoding_rs::Encoding) -> Option<(String, i32)> {
    let (cow, _, had_errors) = encoding.decode(bytes);
    if had_errors {
        return None;
    }
    let text = cow.into_owned();
    let score = decode_quality_score(&text);
    Some((text, score))
}

/// 读取文本文件，自动检测编码
///
/// OOM 防护：
/// - 小于 MAX_TEXT_FILE_STREAM_MB 的文件一次性读取
/// - 超过阈值的文件逐行流式读取，避免一次性加载全部内容到内存
///
/// 编码检测策略：
/// 1. UTF-8（直接尝试）
/// 2. UTF-16 BOM（最明确，无歧义）
/// 3. 对模糊编码（Shift_JIS/EUC-KR/GBK/Big5），尝试所有候选，
///    用解码质量评分选择最优结果
/// 4. latin-1 兜底
fn read_text(file_path: &str) -> Result<String> {
    let metadata = std::fs::metadata(file_path)?;
    let file_size_mb = metadata.len() / 1024 / 1024;

    if file_size_mb > MAX_TEXT_FILE_STREAM_MB {
        // 大文件：流式逐行读取（仅支持 UTF-8）
        return read_text_streaming(file_path);
    }

    let bytes = std::fs::read(file_path)?;

    // 尝试 UTF-8（直接消耗 bytes，失败时通过 into_bytes 拿回，避免 clone）
    match String::from_utf8(bytes) {
        Ok(text) => Ok(text),
        Err(e) => {
            let bytes = e.into_bytes();

            // 检测 UTF-16 BOM（最明确，优先）
            if bytes.len() >= 2 {
                if bytes[0..2] == [0xFF, 0xFE] {
                    let (cow, _, _) = encoding_rs::UTF_16LE.decode(&bytes[2..]);
                    return Ok(cow.into_owned());
                }
                if bytes[0..2] == [0xFE, 0xFF] {
                    let (cow, _, _) = encoding_rs::UTF_16BE.decode(&bytes[2..]);
                    return Ok(cow.into_owned());
                }
            }

            // 模糊编码集合：尝试所有候选，用质量评分选最优
            let candidates: Vec<&'static encoding_rs::Encoding> = vec![
                &encoding_rs::SHIFT_JIS,
                &encoding_rs::EUC_KR,
                &encoding_rs::GBK,
                &encoding_rs::BIG5,
            ];

            let mut best: Option<(String, i32)> = None;
            for enc in candidates {
                if let Some((text, score)) = try_decode(&bytes, enc)
                    && best
                        .as_ref()
                        .is_none_or(|(_, best_score)| score > *best_score)
                {
                    best = Some((text, score));
                }
            }

            if let Some((text, _)) = best {
                return Ok(text);
            }

            // 兜底：latin-1 (逐字节映射)
            let text: String = bytes.iter().map(|&b| b as char).collect();
            Ok(sanitize_garbled(&text))
        }
    }
}

/// 大文件流式读取：逐行读取并收集，避免一次性加载
///
/// 策略：逐行读取为 UTF-8，如果失败则逐行尝试 GBK 解码
fn read_text_streaming(file_path: &str) -> Result<String> {
    let file = std::fs::File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut lines = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(|e| AppError::Encoding(format!("读取行失败: {}", e)))?;
        lines.push(line);
    }

    Ok(lines.join("\n"))
}

/// 读取 PDF 文件
fn read_pdf(file_path: &str) -> Result<String> {
    let _path = Path::new(file_path);
    let file_mb = std::fs::metadata(file_path)
        .map(|m| m.len() / 1024 / 1024)
        .unwrap_or(0);

    let scan = inspect_pdf(Path::new(file_path), PDF_FALLBACK_MIN_TEXT)?;
    let is_scanned = scan.requires_ocr;
    eprintln!(
        "  ✓ PDF 探测: {} | {}页 | 密度 {:.1} 字/页 | 置信度 {}%",
        match scan.status {
            PdfStatus::HasText => "文字层",
            PdfStatus::NeedOcr => "需 OCR",
            PdfStatus::HandDrawn => "手绘/矢量",
            PdfStatus::Error => "异常",
        },
        scan.pages,
        scan.text_density,
        scan.confidence
    );

    if file_mb > PDF_LARGE_FILE_MB {
        return read_pdf_split(file_path, is_scanned);
    }

    read_pdf_single(file_path, is_scanned)
}

/// 单 PDF 转换（三层 fallback）
fn read_pdf_single(file_path: &str, is_scanned: bool) -> Result<String> {
    if is_scanned {
        return read_pdf_scan(file_path);
    }

    // 第一层：MarkItDown（保留标题结构）
    if let Ok(text) = read_markitdown(file_path)
        && text.trim().len() >= PDF_FALLBACK_MIN_TEXT
    {
        let cleaned = crate::quality::clean_text(&text);
        // 检查是否保留了标题结构，没有则回退到 PyMuPDF（带 TOC 注入）
        let has_structure = crate::pipeline::heading_regex().is_match(&cleaned);
        if has_structure {
            return Ok(cleaned);
        }
        // 无标题结构，继续尝试下层
    }

    // 第二层：PyMuPDF（带 TOC 标题注入 + 预处理）
    if let Ok(text) = read_pdf_raw(file_path)
        && text.trim().len() >= PDF_FALLBACK_MIN_TEXT
    {
        return Ok(text);
    }

    // 最终 fallback：MinerU
    read_pdf_scan(file_path)
}

/// PyMuPDF 裸提取（带 TOC 标题注入）
fn read_pdf_raw(file_path: &str) -> Result<String> {
    let output = Command::new("python3")
        .arg("-c")
        .arg(
            r###"
import pymupdf, sys, re

doc = pymupdf.open(sys.argv[1])
toc = doc.get_toc()

# 建立页码到标题列表的映射
page_headings = {}
for level, title, page in toc:
    idx = page - 1  # 转为 0-based
    if 0 <= idx < doc.page_count:
        if idx not in page_headings:
            page_headings[idx] = []
        page_headings[idx].append((level, title))

pages = []
for i in range(doc.page_count):
    text = doc[i].get_text().strip()
    if not text:
        continue

    # 注入 TOC 标题（如果该页有目录条目且页面上没有）
    if i in page_headings:
        for level, title in sorted(page_headings[i], key=lambda x: x[0]):
            # 避免重复注入：如果页面上已有该标题文字，跳过
            if title in text:
                continue
            prefix = "#" * min(level, 6)
            pages.append(f"{prefix} {title}")

    # 添加页码标记作为二级标题（至少保证按页分块）
    pages.append(f"## 第 {i + 1} 页\n\n{text}")

doc.close()
print("\n\n".join(pages))
"###,
        )
        .arg(file_path)
        .output()
        .map_err(|e| AppError::Conversion(format!("PyMuPDF 提取失败: {}", e)))?;

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    if text.trim().is_empty() {
        return Err(AppError::Conversion("PyMuPDF 未获取到任何文本".to_string()));
    }
    // 应用文本预处理
    let cleaned = crate::quality::clean_text(&text);

    // 页眉页脚清理
    let result = crate::quality::remove_headers_footers(&cleaned);
    if !result.headers_removed.is_empty() || !result.footers_removed.is_empty() {
        eprintln!(
            "  ✓ 页眉页脚清理: 移除 {} 个页眉, {} 个页脚 (共 {} 页)",
            result.headers_removed.len(),
            result.footers_removed.len(),
            result.pages_processed
        );
    }

    Ok(result.cleaned_text)
}

/// 扫描版 PDF → OCR 提取
///
/// 尝试顺序：
///   1. MinerU（首选，质量最好）
///   2. pdf2image + pytesseract（fallback，需安装 tesseract）
///
/// 查找 mineru 可执行文件路径
fn find_mineru() -> Option<String> {
    // 1. 检查 $MINERU_PATH 环境变量
    if let Ok(path) = std::env::var("MINERU_PATH")
        && std::path::Path::new(&path).exists()
    {
        return Some(path);
    }

    // 2. 尝试 PATH 中的 mineru
    if let Ok(out) = Command::new("which").arg("mineru").output()
        && out.status.success()
    {
        return Some(String::from_utf8_lossy(&out.stdout).trim().to_string());
    }

    // 3. 尝试标准安装位置（相对于 home）
    if let Ok(home) = std::env::var("HOME") {
        let candidates = [
            format!("{}/.venv/bin/mineru", home),
            format!("{}/.local/bin/mineru", home),
            format!("{}/venv/bin/mineru", home),
        ];
        for path in &candidates {
            if std::path::Path::new(path).exists() {
                return Some(path.clone());
            }
        }
    }

    // 4. 尝试系统标准路径
    let system_paths = ["/usr/local/bin/mineru", "/opt/mineru/bin/mineru"];
    for path in &system_paths {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    None
}

/// 使用 pdf-expert-batch-ocr 进行 OCR（macOS + PDF Expert）
///
/// 流程：创建临时队列 → 调用 batch_ocr.py → 读取输出
fn read_pdf_expert_ocr(file_path: &str) -> Result<String> {
    let ocr_project = match find_ocr_project() {
        Some(p) => p,
        None => {
            eprintln!("  ℹ pdf-expert-batch-ocr 项目未找到，转到 MinerU");
            return read_pdf_scan_fallback_mineru(file_path);
        }
    };

    // 检查必要的文件
    let batch_ocr_script = ocr_project.join("batch_ocr.py");
    if !batch_ocr_script.exists() {
        eprintln!("  ℹ batch_ocr.py 不存在，转到 MinerU");
        return read_pdf_scan_fallback_mineru(file_path);
    }

    // 验证环境：Python 3
    let python_check = Command::new("python3").arg("--version").output();
    if python_check.is_err() {
        eprintln!("  ℹ Python 3 不可用，转到 MinerU");
        return read_pdf_scan_fallback_mineru(file_path);
    }

    // 创建临时工作目录
    let temp_dir = tempfile::tempdir()
        .map_err(|e| AppError::Conversion(format!("创建临时目录失败: {}", e)))?;
    let temp_path = temp_dir.path();
    let queue_path = temp_path.join("ocr_queue.json");
    let output_dir = temp_path.join("output");
    std::fs::create_dir(&output_dir)
        .map_err(|e| AppError::Conversion(format!("创建输出目录失败: {}", e)))?;

    // 生成 queue.json
    let queue_json = serde_json::json!({
        "queue": [{"path": file_path}]
    });
    let queue_content = serde_json::to_string_pretty(&queue_json)
        .map_err(|e| AppError::Conversion(format!("生成 queue JSON 失败: {}", e)))?;
    std::fs::write(&queue_path, queue_content)
        .map_err(|e| AppError::Conversion(format!("写入 queue.json 失败: {}", e)))?;

    // 调用 batch_ocr.py
    eprintln!("  → 调用 pdf-expert-batch-ocr...");
    let result = Command::new("python3")
        .args([
            batch_ocr_script.to_str().unwrap_or("batch_ocr.py"),
            "--queue",
            queue_path.to_str().unwrap_or(""),
            "--output-dir",
            output_dir.to_str().unwrap_or(""),
        ])
        .current_dir(&ocr_project)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            AppError::Conversion(format!(
                "pdf-expert-batch-ocr 调用失败: {}\n转到 MinerU 重试",
                e
            ))
        });

    match result {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            eprintln!("  ℹ pdf-expert-batch-ocr 失败: {}\n  转到 MinerU", stderr);
            return read_pdf_scan_fallback_mineru(file_path);
        }
        Err(e) => {
            eprintln!("  ℹ pdf-expert-batch-ocr 执行错误: {}\n  转到 MinerU", e);
            return read_pdf_scan_fallback_mineru(file_path);
        }
    }

    // 从输出目录查找最大的 markdown 文件
    let mut md_files: Vec<PathBuf> = walkdir::WalkDir::new(&output_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "md").unwrap_or(false))
        .filter(|e| e.metadata().map(|m| m.len() > 10).unwrap_or(false))
        .map(|e| e.path().to_path_buf())
        .collect();

    if md_files.is_empty() {
        eprintln!("  ℹ OCR 输出中未找到 markdown 文件，转到 MinerU");
        return read_pdf_scan_fallback_mineru(file_path);
    }

    // 选择最大的文件
    md_files.sort_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0));
    md_files.reverse();

    let text = read_text(
        md_files[0]
            .to_str()
            .ok_or_else(|| AppError::Conversion("OCR 结果路径包含非法字符".to_string()))?,
    )?;

    eprintln!("  ✓ pdf-expert-batch-ocr OCR 成功");
    Ok(crate::quality::clean_text(&text))
}

/// MinerU fallback（当 pdf-expert-batch-ocr 不可用时）
fn read_pdf_scan_fallback_mineru(file_path: &str) -> Result<String> {
    // 先尝试 MinerU
    let mineru_path = find_mineru();

    let mineru_path = match mineru_path {
        Some(p) => p,
        None => return read_pdf_ocr_fallback(file_path),
    };

    let temp_dir = tempfile::tempdir()
        .map_err(|e| AppError::Conversion(format!("创建临时目录失败: {}", e)))?;

    let result = Command::new(&mineru_path)
        .args([
            "-p",
            file_path,
            "-o",
            &temp_dir.path().to_string_lossy(),
            "-l",
            "ch",
            "-b",
            "pipeline",
            "-m",
            "ocr",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            AppError::Conversion(format!(
                "MinerU 转换失败 (预期 {} 秒内完成): {}\n{}",
                PDF_CONVERT_TIMEOUT,
                e,
                crate::scan::ocr_guidance_for_file(file_path)
            ))
        })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(AppError::Conversion(format!(
            "MinerU 转换失败: {}{}",
            stderr,
            crate::scan::ocr_guidance_for_file(file_path)
        )));
    }

    // 找最大的 .md 文件
    let mut md_files: Vec<PathBuf> = walkdir::WalkDir::new(temp_dir.path())
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "md").unwrap_or(false))
        .filter(|e| e.metadata().map(|m| m.len() > 10).unwrap_or(false))
        .map(|e| e.path().to_path_buf())
        .collect();

    md_files.sort_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0));
    md_files.reverse();

    if md_files.is_empty() {
        return Err(AppError::Conversion(format!(
            "MinerU 转换后未找到有效的 Markdown 文件{}",
            crate::scan::ocr_guidance_for_file(file_path)
        )));
    }

    let text = read_text(
        md_files[0]
            .to_str()
            .ok_or_else(|| AppError::Conversion("PDF 结果路径包含非法字符".to_string()))?,
    )?;
    Ok(crate::quality::clean_text(&text))
}

fn read_pdf_scan(file_path: &str) -> Result<String> {
    // 优先尝试 pdf-expert-batch-ocr（若可用）
    if let Ok(text) = read_pdf_expert_ocr(file_path) {
        return Ok(text);
    }

    // fallback: MinerU
    let mineru_path = find_mineru();

    let mineru_path = match mineru_path {
        Some(p) => p,
        None => return read_pdf_ocr_fallback(file_path),
    };

    let temp_dir = tempfile::tempdir()
        .map_err(|e| AppError::Conversion(format!("创建临时目录失败: {}", e)))?;

    let result = Command::new(&mineru_path)
        .args([
            "-p",
            file_path,
            "-o",
            &temp_dir.path().to_string_lossy(),
            "-l",
            "ch",
            "-b",
            "pipeline",
            "-m",
            "ocr",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            AppError::Conversion(format!(
                "MinerU 转换失败 (预期 {} 秒内完成): {}\n{}",
                PDF_CONVERT_TIMEOUT,
                e,
                crate::scan::ocr_guidance_for_file(file_path)
            ))
        })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(AppError::Conversion(format!(
            "MinerU 转换失败: {}{}",
            stderr,
            crate::scan::ocr_guidance_for_file(file_path)
        )));
    }

    // 找最大的 .md 文件
    let mut md_files: Vec<PathBuf> = walkdir::WalkDir::new(temp_dir.path())
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "md").unwrap_or(false))
        .filter(|e| e.metadata().map(|m| m.len() > 10).unwrap_or(false))
        .map(|e| e.path().to_path_buf())
        .collect();

    md_files.sort_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0));
    md_files.reverse();

    if md_files.is_empty() {
        return Err(AppError::Conversion(format!(
            "MinerU 转换后未找到有效的 Markdown 文件{}",
            crate::scan::ocr_guidance_for_file(file_path)
        )));
    }

    let text = read_text(
        md_files[0]
            .to_str()
            .ok_or_else(|| AppError::Conversion("PDF 结果路径包含非法字符".to_string()))?,
    )?;
    Ok(crate::quality::clean_text(&text))
}

/// OCR fallback：使用 pdf2image + pytesseract
///
/// 先检查依赖是否可用，不可用则返回清晰的安装指引。
fn read_pdf_ocr_fallback(file_path: &str) -> Result<String> {
    // 检查 pytesseract 是否可用
    let check = Command::new("python3")
        .arg("-c")
        .arg("import pytesseract, pdf2image; print('OK')")
        .output();

    let has_deps = matches!(
        &check,
        Ok(out) if String::from_utf8_lossy(&out.stdout).contains("OK")
    );

    if !has_deps {
        let mut msg = "扫描版/图片 PDF 需要 OCR 工具。\n".to_string();
        msg.push_str("  方案1（推荐）: uv pip install -U 'mineru[core]'\n");
        msg.push_str("  方案2: brew install tesseract && uv pip install pytesseract pdf2image\n");
        msg.push_str(&crate::scan::ocr_guidance_for_file(file_path));
        return Err(AppError::Conversion(msg));
    }

    let temp_dir = tempfile::tempdir()
        .map_err(|e| AppError::Conversion(format!("创建临时目录失败: {}", e)))?;

    let output = Command::new("python3")
        .arg("-c")
        .arg(
            r###"
import sys, os
from pdf2image import convert_from_path
import pytesseract

pdf_path = sys.argv[1]
out_path = sys.argv[2]

from pdf2image.pdf2image import pdfinfo_from_path
info = pdfinfo_from_path(pdf_path)
total_pages = int(info.get('Pages', 0))
if total_pages > 5:
    raise RuntimeError(
        f"Tesseract fallback 只适合抽样验证：该 PDF 共 {total_pages} 页，"
        "不能只 OCR 前 5 页后进入完整编译。请安装 MinerU 或先完成全文 OCR。"
    )

pages = convert_from_path(pdf_path, first_page=1, last_page=total_pages)
results = []
for i, page in enumerate(pages):
    text = pytesseract.image_to_string(page, lang='chi_sim+eng')
    if text.strip():
        results.append(f"## Page {i+1}\n\n{text.strip()}")

with open(out_path, 'w', encoding='utf-8') as f:
    f.write("\n\n".join(results))
print("OK")
"###,
        )
        .arg(file_path)
        .arg(temp_dir.path().join("ocr_output.md"))
        .output()
        .map_err(|e| AppError::Conversion(format!("OCR 转换失败: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Conversion(format!(
            "OCR 处理失败（多页 PDF 请安装 MinerU 或先完成全文 OCR；Tesseract fallback 不再抽样冒充全文结果）: {}",
            stderr
        )));
    }

    let out_file = temp_dir.path().join("ocr_output.md");
    let text = std::fs::read_to_string(&out_file)
        .map_err(|e| AppError::Conversion(format!("读取 OCR 结果失败: {}", e)))?;

    if text.trim().is_empty() {
        return Err(AppError::Conversion(
            "OCR 未能识别到任何文本。该 PDF 可能为纯图片或加密文档。".to_string(),
        ));
    }

    Ok(crate::quality::clean_text(&text))
}

/// 大 PDF 拆分处理
///
/// OOM 防护：
/// - 超过 100MB 的 PDF 直接跳过整体转换，强制按 TOC 拆分
/// - 避免将整个大文件加载到内存中处理
fn read_pdf_split(file_path: &str, is_scanned: bool) -> Result<String> {
    let file_mb = std::fs::metadata(file_path)
        .map(|m| m.len() / 1024 / 1024)
        .unwrap_or(0);

    // OOM 防护：超过 100MB 的 PDF 不尝试整体转换，直接拆分
    if file_mb <= 100 {
        // 策略 1: 先尝试整体转换（仅对"中等大"文件）
        if let Ok(full_md) = read_pdf_single(file_path, is_scanned) {
            return Ok(split_markdown_by_headings(&full_md));
        }
    }

    // 策略 2: 按 TOC 拆分
    let segments = split_pdf_by_toc(file_path)?;

    let temp_dir = tempfile::tempdir()
        .map_err(|e| AppError::Conversion(format!("创建临时目录失败: {}", e)))?;

    let mut parts = Vec::new();
    for (idx, (title, start, end)) in segments.iter().enumerate() {
        let seg_path = temp_dir.path().join(format!("seg_{:03}.pdf", idx + 1));
        let seg_path_str = seg_path.to_string_lossy().to_string();
        save_pdf_range(file_path, &seg_path_str, *start, *end)?;
        let seg_md = read_pdf_single(&seg_path_str, is_scanned)?;
        parts.push(format!("# {}\n\n{}\n", title, seg_md.trim()));
    }

    Ok(parts.join("\n\n"))
}

/// 按 Markdown 标题拆分
fn split_markdown_by_headings(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut parts: Vec<Vec<&str>> = Vec::new();
    let mut current: Vec<&str> = Vec::new();

    let heading_re = &HEADING_PREFIX_RE;

    for line in &lines {
        if heading_re.is_match(line) && !current.is_empty() {
            parts.push(current);
            current = vec![line];
        } else {
            current.push(line);
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    if parts.is_empty() {
        return text.to_string();
    }

    parts
        .into_iter()
        .map(|p| p.join("\n"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// 按 TOC 拆分 PDF
/// 按 TOC 拆分 PDF，返回 (章节标题, 起始页, 结束页) 列表
pub fn split_pdf_by_toc(file_path: &str) -> Result<Vec<(String, usize, usize)>> {
    let output = Command::new("python3")
        .arg("-c")
        .arg(
            r##"
import pymupdf, json, sys
doc = pymupdf.open(sys.argv[1])
total = doc.page_count
toc = doc.get_toc()
doc.close()

top_level = [(title, page) for lvl, title, page in toc if lvl == 1]
min_entries = int(sys.argv[2])
pages_per = int(sys.argv[3])

if len(top_level) >= min_entries:
    segments = []
    for i, (title, start_1based) in enumerate(top_level):
        start = max(start_1based - 1, 0)
        end = top_level[i + 1][1] - 2 if i + 1 < len(top_level) else total - 1
        if end < start:
            end = start
        segments.append((title, start, end))
    print(json.dumps(segments))
else:
    segments = []
    for i in range(0, total, pages_per):
        end = min(i + pages_per - 1, total - 1)
        segments.append((f"第 {i+1}-{end+1} 页", i, end))
    print(json.dumps(segments))
"##,
        )
        .arg(file_path)
        .arg(PDF_TOC_MIN_ENTRIES.to_string())
        .arg(PDF_SPLIT_PAGES_DEFAULT.to_string())
        .output()
        .map_err(|e| AppError::Conversion(format!("TOC 拆分失败: {}", e)))?;

    let json_str = String::from_utf8_lossy(&output.stdout);
    let segments: Vec<(String, usize, usize)> = serde_json::from_str(&json_str)
        .map_err(|e| AppError::Conversion(format!("TOC 解析失败: {}", e)))?;

    Ok(segments)
}

/// 保存 PDF 页面范围
/// 保存 PDF 页面范围到临时文件
pub fn save_pdf_range(src: &str, dst: &str, start: usize, end: usize) -> Result<()> {
    let output = Command::new("python3")
        .arg("-c")
        .arg(
            r##"
import pymupdf, sys
src = pymupdf.open(sys.argv[1])
dst = pymupdf.open()
dst.insert_pdf(src, from_page=int(sys.argv[3]), to_page=int(sys.argv[4]))
dst.save(sys.argv[2], garbage=3, deflate=True)
dst.close()
src.close()
"##,
        )
        .arg(src)
        .arg(dst)
        .arg(start.to_string())
        .arg(end.to_string())
        .output()
        .map_err(|e| AppError::Conversion(format!("PDF 拆分保存失败: {}", e)))?;

    if !output.status.success() {
        return Err(AppError::Conversion(format!(
            "PDF 拆分保存失败: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    Ok(())
}

/// MarkItDown 统一转换
fn read_markitdown(file_path: &str) -> Result<String> {
    let output = Command::new("python3")
        .arg("-c")
        .arg(
            r##"
from markitdown import MarkItDown
import sys
try:
    md = MarkItDown(enable_plugins=False)
except TypeError:
    md = MarkItDown()
result = md.convert(sys.argv[1])
print(result.text_content or "")
"##,
        )
        .arg(file_path)
        .output()
        .map_err(|e| {
            AppError::Conversion(format!(
                "MarkItDown 未安装或运行失败: {}。运行: uv pip install 'markitdown[all]'",
                e
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Conversion(format!(
            "MarkItDown 转换失败: {}",
            stderr
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// 清理 latin-1 乱码
fn sanitize_garbled(text: &str) -> String {
    text.chars()
        .map(|ch| {
            let code = ch as u32;
            match code {
                0x0A | 0x0D | 0x09 => ch,
                32..=126 => ch,
                0x4E00..=0x9FFF => ch,
                0x3000..=0x303F => ch,
                0xFF00..=0xFFEF => ch,
                0xFFFD => ' ',
                0..32 => ' ',
                _ => ch,
            }
        })
        .collect()
}

/// 猜测文档标题
pub fn guess_title(document: &str, file_name: &str) -> String {
    let heading_re = &H1_RE;
    let heading2_re = &H2_RE;

    for line in document.lines().take(10) {
        let line = line.trim();
        if let Some(caps) = heading_re.captures(line) {
            return caps[1].trim().to_string();
        }
        if let Some(caps) = heading2_re.captures(line) {
            return caps[1].trim().to_string();
        }
    }

    if !file_name.is_empty() {
        Path::new(file_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("未命名")
            .to_string()
    } else {
        "未命名".to_string()
    }
}

/// 带超时的 Markdown 转换（异步包装）
/// 根据文件估算超时时间（秒）
/// 扫描版PDF OCR 每页约 60 秒，文本版每页约 1 秒
fn estimate_timeout(file_path: &str) -> u64 {
    let pages = estimate_page_count(file_path);
    let base = if is_likely_scanned(file_path) {
        pages * 90 // 扫描版：每页90秒（含模型加载）
    } else {
        pages * 2 // 文本版：每页2秒
    };
    base.clamp(60, OCR_TIMEOUT_SECS)
}

/// 估算PDF页数（非PDF文件返回默认值）
fn estimate_page_count(file_path: &str) -> u64 {
    let path = std::path::Path::new(file_path);
    let suffix = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if suffix != "pdf" {
        return 10; // 非PDF默认10页
    }
    // 尝试从文件大小估算：平均每页约 200KB
    if let Ok(meta) = std::fs::metadata(file_path) {
        let kb = meta.len() / 1024;
        (kb / 200).clamp(1, 1000)
    } else {
        10
    }
}

/// 粗略判断是否为扫描版PDF
/// 策略：纯图片PDF通常很大（>2MB），文本版PDF通常较小（<1MB）
fn is_likely_scanned(file_path: &str) -> bool {
    let path = std::path::Path::new(file_path);
    let suffix = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if suffix != "pdf" {
        return false;
    }
    // 超过 2MB 的PDF大概率是扫描版（纯文本PDF通常几百KB）
    if let Ok(meta) = std::fs::metadata(file_path) {
        meta.len() > 2 * 1024 * 1024
    } else {
        false
    }
}

pub async fn convert_to_markdown_async(file_path: &str) -> Result<String> {
    convert_to_markdown_async_with_timeout(file_path, None).await
}

/// 带自定义超时的异步转换（用于 quality 等需要快速反馈的场景）
pub async fn convert_to_markdown_async_with_timeout(
    file_path: &str,
    custom_timeout_secs: Option<u64>,
) -> Result<String> {
    let path = file_path.to_string();
    let timeout_secs = custom_timeout_secs.unwrap_or_else(|| estimate_timeout(file_path));
    tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        tokio::task::spawn_blocking(move || convert_to_markdown(&path)),
    )
    .await
    .map_err(|_| {
        let mut msg = format!(
            "文件转换超时 ({} 秒): {}\n提示：扫描版/图片PDF需要OCR处理，耗时较长。",
            timeout_secs, file_path
        );
        if is_likely_scanned(file_path) {
            msg.push_str(&crate::scan::ocr_guidance_for_file(file_path));
        }
        AppError::Timeout(msg)
    })?
    .map_err(|e| AppError::TaskPanic(format!("转换任务 panic: {}", e)))?
}

/// 书籍元数据
#[derive(Debug, Clone, Default)]
pub struct BookMetadata {
    pub title: String,
    pub author: String,
    pub publisher: String,
    pub isbn: String,
    pub page_count: usize,
}

/// 从 PDF 提取书籍元数据
///
/// 提取策略（按优先级）：
/// 1. PDF Info 字典（Title, Author 等）—— 最准确
/// 2. 文本正则匹配（版权页）—— fallback
/// 3. 文件名 —— 最后手段
///
/// 书名提取策略（按优先级）：
/// 1. PDF Info 字典的 Title 字段（作者/出版者显式设置的元数据，最权威）
/// 2. 文件名（用户已命名，次优 fallback）
///
/// 不尝试从文本内容中猜测书名——文本搜索误报率极高，
/// 且文件名通常已包含正确书名。
pub fn extract_pdf_metadata(file_path: &str) -> BookMetadata {
    let mut meta = BookMetadata::default();

    // 策略1：从 PDF Info 字典提取（最权威来源）
    if let Ok(doc) = lopdf::Document::load(file_path) {
        meta.page_count = doc.get_pages().len();

        if let Ok(info_ref) = doc.trailer.get(b"Info")
            && let lopdf::Object::Reference(id) = info_ref
            && let Ok(info_obj) = doc.get_object(*id)
            && let Ok(info_dict) = info_obj.as_dict()
        {
            // Title：仅当非空且不是页码标记时才采用
            if let Ok(obj) = info_dict.get(b"Title") {
                let raw = pdf_string_value(obj);
                if !raw.trim().is_empty() && !looks_like_page_marker(&raw) {
                    meta.title = raw;
                }
            }
            // Author
            if let Ok(obj) = info_dict.get(b"Author") {
                meta.author = pdf_string_value(obj);
            }
            // Subject / Publisher
            if let Ok(obj) = info_dict.get(b"Subject") {
                meta.publisher = pdf_string_value(obj);
            }
        }
    }

    // 策略2：Info 字典无有效 Title 时，直接用文件名
    // 文件名是用户显式命名的，比文本搜索更可靠
    if meta.title.is_empty() {
        meta.title = Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
    }

    meta
}

/// 从 lopdf Object 中提取字符串值
fn pdf_string_value(obj: &lopdf::Object) -> String {
    match obj {
        lopdf::Object::String(s, _) => String::from_utf8_lossy(s).to_string(),
        lopdf::Object::Name(n) => String::from_utf8_lossy(n).to_string(),
        _ => String::new(),
    }
}

/// 启发式校验：判断字符串是否看起来像页码/章节标记而非真正的书名
///
/// 常见误判模式：
/// - "## 第 1 页"、"第 1 章"、"第一章"
/// - "Page 1"、"Chapter 1"
/// - "1"、"01" 等纯数字
fn looks_like_page_marker(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return true;
    }

    // 模式1：以 # 开头（Markdown 标题标记）
    if trimmed.starts_with('#') {
        return true;
    }

    // 模式2：包含 "第 x 页" / "第 x 章"
    if PAGE_CHAPTER_RE.is_match(trimmed) {
        return true;
    }

    // 模式3：英文 Page / Chapter + 数字
    if ENG_MARKER_RE.is_match(trimmed) {
        return true;
    }

    // 模式4：纯数字（如 "1"、"01"）
    if trimmed.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }

    // 模式5：极短字符串（< 3 字符，不太可能是书名）
    if trimmed.chars().count() < 3 {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_garbled_ascii() {
        let input = "Hello World";
        assert_eq!(sanitize_garbled(input), "Hello World");
    }

    #[test]
    fn test_sanitize_garbled_cjk() {
        let input = "你好世界";
        assert_eq!(sanitize_garbled(input), "你好世界");
    }

    #[test]
    fn test_sanitize_garbled_replacement_char() {
        let input = "a\u{FFFD}b";
        assert_eq!(sanitize_garbled(input), "a b");
    }

    #[test]
    fn test_sanitize_garbled_control_chars() {
        let input = "a\x00b\x01c";
        assert_eq!(sanitize_garbled(input), "a b c");
    }

    #[test]
    fn test_sanitize_garbled_mixed() {
        let input = "Hello\n世界\x00\u{FFFD}123";
        // \x00 和 \u{FFFD} 都被替换为空格，形成两个连续空格
        assert_eq!(sanitize_garbled(input), "Hello\n世界  123");
    }

    #[test]
    fn test_guess_title_from_h1() {
        let doc = "# My Title\n\nSome content";
        assert_eq!(guess_title(doc, "file.md"), "My Title");
    }

    #[test]
    fn test_guess_title_from_h2() {
        let doc = "\n\n## Section Title\n\nContent";
        assert_eq!(guess_title(doc, "file.md"), "Section Title");
    }

    #[test]
    fn test_guess_title_fallback_to_filename() {
        let doc = "No heading here\nJust content";
        assert_eq!(guess_title(doc, "/path/to/my_doc.md"), "my_doc");
    }

    #[test]
    fn test_guess_title_empty_doc() {
        let doc = "";
        assert_eq!(guess_title(doc, ""), "未命名");
    }

    #[test]
    fn test_is_text_format() {
        assert!(is_text_format(".md"));
        assert!(is_text_format(".txt"));
        assert!(is_text_format(".markdown"));
        assert!(!is_text_format(".pdf"));
        assert!(!is_text_format(".docx"));
    }

    #[test]
    fn test_is_pdf_format() {
        assert!(is_pdf_format(".pdf"));
        assert!(!is_pdf_format(".md"));
    }

    #[test]
    fn test_is_other_format() {
        assert!(is_other_format(".docx"));
        assert!(is_other_format(".html"));
        assert!(!is_other_format(".md"));
    }

    // ── 边界测试 ──

    #[test]
    fn test_guess_title_no_headers() {
        let doc = "内容A\n\n内容B";
        assert_eq!(guess_title(doc, "文件A.md"), "文件A");
    }

    #[test]
    fn test_sanitize_garbled_empty() {
        let result = sanitize_garbled("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_sanitize_garbled_all_control() {
        let input = "\x00\x01\x02\x03\x04\x05";
        let result = sanitize_garbled(input);
        assert!(!result.contains('\x00'));
        assert!(!result.contains('\x01'));
    }

    #[test]
    fn test_guess_title_whitespace_only() {
        let doc = "   \n\n   ";
        assert_eq!(guess_title(doc, "文件B.md"), "文件B");
    }

    // ── OOM 防护边界测试 ──

    #[test]
    fn test_check_file_size_small_file_ok() {
        use std::io::Write;
        let tmp = std::env::temp_dir().join("cardnote_test_small.txt");
        let mut f = std::fs::File::create(&tmp).unwrap();
        f.write_all(b"small content").unwrap();
        drop(f);

        let result = check_file_size(&tmp);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0); // < 1MB

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_check_file_size_nonexistent() {
        let tmp = std::env::temp_dir().join("cardnote_test_nonexistent_xyz");
        let result = check_file_size(&tmp);
        assert!(result.is_err());
    }

    #[test]
    fn test_convert_null_byte_path() {
        let result = convert_to_markdown("/tmp/foo\0bar");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("非法字符") || err_msg.contains("不存在"));
    }

    #[test]
    fn test_convert_nonexistent_file() {
        let result = convert_to_markdown("/tmp/cardnote_nonexistent_file_xyz.md");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("不存在"));
    }

    #[test]
    fn test_read_text_streaming_basic() {
        use std::io::Write;
        let tmp = std::env::temp_dir().join("cardnote_test_stream.txt");
        let mut f = std::fs::File::create(&tmp).unwrap();
        writeln!(f, "行A").unwrap();
        writeln!(f, "行B").unwrap();
        writeln!(f, "行C").unwrap();
        drop(f);

        let result = read_text_streaming(tmp.to_str().unwrap()).unwrap();
        assert!(result.contains("行A"));
        assert!(result.contains("行B"));
        assert!(result.contains("行C"));

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_read_text_streaming_empty_file() {
        use std::io::Write;
        let tmp = std::env::temp_dir().join("cardnote_test_stream_empty.txt");
        let mut f = std::fs::File::create(&tmp).unwrap();
        f.write_all(b"").unwrap();
        drop(f);

        let result = read_text_streaming(tmp.to_str().unwrap()).unwrap();
        assert_eq!(result, "");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_read_text_streaming_unicode() {
        use std::io::Write;
        let tmp = std::env::temp_dir().join("cardnote_test_stream_unicode.txt");
        let mut f = std::fs::File::create(&tmp).unwrap();
        writeln!(f, "内容C").unwrap();
        writeln!(f, "English text").unwrap();
        writeln!(f, "テキストA").unwrap();
        drop(f);

        let result = read_text_streaming(tmp.to_str().unwrap()).unwrap();
        assert!(result.contains("内容C"));
        assert!(result.contains("English text"));
        assert!(result.contains("テキストA"));

        let _ = std::fs::remove_file(&tmp);
    }

    // ── 编码检测边界测试 ──

    #[test]
    fn test_read_text_utf16_le_bom() {
        use std::io::Write;
        let tmp = std::env::temp_dir().join("cardnote_test_utf16le.txt");
        let mut f = std::fs::File::create(&tmp).unwrap();
        // UTF-16 LE BOM + "内容D内容D" in UTF-16 LE
        let mut data = vec![0xFF, 0xFE];
        for ch in "内容D内容D".encode_utf16() {
            data.extend_from_slice(&ch.to_le_bytes());
        }
        f.write_all(&data).unwrap();
        drop(f);

        let result = read_text(tmp.to_str().unwrap()).unwrap();
        assert!(result.contains("内容D内容D"));

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_read_text_utf16_be_bom() {
        use std::io::Write;
        let tmp = std::env::temp_dir().join("cardnote_test_utf16be.txt");
        let mut f = std::fs::File::create(&tmp).unwrap();
        // UTF-16 BE BOM + "内容E内容E" in UTF-16 BE
        let mut data = vec![0xFE, 0xFF];
        for ch in "内容E内容E".encode_utf16() {
            data.extend_from_slice(&ch.to_be_bytes());
        }
        f.write_all(&data).unwrap();
        drop(f);

        let result = read_text(tmp.to_str().unwrap()).unwrap();
        assert!(result.contains("内容E内容E"));

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_read_text_shift_jis() {
        use std::io::Write;
        let tmp = std::env::temp_dir().join("cardnote_test_sjis.txt");
        let mut f = std::fs::File::create(&tmp).unwrap();
        // "テスト" in Shift_JIS
        f.write_all(&[0x83, 0x65, 0x83, 0x58, 0x83, 0x67]).unwrap();
        drop(f);

        let result = read_text(tmp.to_str().unwrap()).unwrap();
        assert!(result.contains("テスト"));

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_read_text_euc_kr() {
        use std::io::Write;
        let tmp = std::env::temp_dir().join("cardnote_test_euckr.txt");
        let mut f = std::fs::File::create(&tmp).unwrap();
        // "테스트" in EUC-KR
        f.write_all(&[0xC5, 0xD7, 0xBD, 0xBA, 0xC6, 0xAE]).unwrap();
        drop(f);

        let result = read_text(tmp.to_str().unwrap()).unwrap();
        assert!(result.contains("테스트"));

        let _ = std::fs::remove_file(&tmp);
    }
}
