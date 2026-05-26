use std::collections::HashMap;

use regex::Regex;

// ═══════════════════════════════════════════════════════
//  1. 字符健康度
// ═══════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct CharacterHealth {
    pub total_chars: usize,
    pub printable_ratio: f32,
    pub cjk_ratio: f32,
    pub garbled_ratio: f32,
    pub replacement_char_count: usize,
    pub score: f32,
}

pub fn character_health(text: &str) -> CharacterHealth {
    let total = text.chars().count();
    if total == 0 {
        return CharacterHealth {
            total_chars: 0,
            printable_ratio: 0.0,
            cjk_ratio: 0.0,
            garbled_ratio: 1.0,
            replacement_char_count: 0,
            score: 0.0,
        };
    }

    let mut printable = 0usize;
    let mut cjk = 0usize;
    let mut garbled = 0usize;
    let mut replacement = 0usize;

    for ch in text.chars() {
        let code = ch as u32;
        let is_printable = matches!(code,
            0x09 | 0x0A | 0x0D |
            0x20..=0x7E |
            0x4E00..=0x9FFF |
            0x3000..=0x303F |
            0xFF00..=0xFFEF |
            0x3400..=0x4DBF |
            0x20000..=0x2A6DF
        );
        if is_printable {
            printable += 1;
        } else {
            garbled += 1;
        }

        if matches!(code, 0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x3000..=0x303F) {
            cjk += 1;
        }
        if code == 0xFFFD {
            replacement += 1;
        }
    }

    let printable_ratio = printable as f32 / total as f32;
    let cjk_ratio = cjk as f32 / total as f32;
    let garbled_ratio = garbled as f32 / total as f32;

    let score = (printable_ratio * 100.0).clamp(0.0, 100.0);

    CharacterHealth {
        total_chars: total,
        printable_ratio,
        cjk_ratio,
        garbled_ratio,
        replacement_char_count: replacement,
        score,
    }
}

// ═══════════════════════════════════════════════════════
//  2. 结构完整性
// ═══════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct StructuralIntegrity {
    pub heading_counts: [usize; 6],
    pub avg_paragraph_length: f32,
    pub paragraph_count: usize,
    pub short_paragraphs: usize,
    pub long_paragraphs: usize,
    pub has_toc: bool,
    pub toc_line_count: usize,
    pub score: f32,
}

pub fn structural_integrity(text: &str) -> StructuralIntegrity {
    let lines: Vec<&str> = text.lines().collect();

    let mut heading_counts = [0usize; 6];
    let heading_re = Regex::new(r"^(#{1,6})\s+").unwrap();

    for line in &lines {
        if let Some(caps) = heading_re.captures(line) {
            let level = caps[1].len();
            if (1..=6).contains(&level) {
                heading_counts[level - 1] += 1;
            }
        }
    }

    let mut paragraphs = Vec::new();
    let mut current = String::new();
    for line in &lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current.is_empty() {
                paragraphs.push(current.trim().to_string());
                current.clear();
            }
        } else {
            current.push_str(trimmed);
            current.push(' ');
        }
    }
    if !current.is_empty() {
        paragraphs.push(current.trim().to_string());
    }

    let paragraph_count = paragraphs.len();
    let total_len: usize = paragraphs.iter().map(|p| p.len()).sum();
    let avg_paragraph_length = if paragraph_count > 0 {
        total_len as f32 / paragraph_count as f32
    } else {
        0.0
    };

    let short_paragraphs = paragraphs.iter().filter(|p| p.len() < 20).count();
    let long_paragraphs = paragraphs.iter().filter(|p| p.len() > 500).count();

    // 检测目录混入：多语言目录格式
    // 与 preprocess::remove_toc_generic 保持一致
    let toc_re = Regex::new(
        r"^(第[\d一二三四五六七八九十百千]+章|[一二三四五六七八九十百千]+[、．.]\s*.*|（[一二三四五六七八九十]）.*|Chapter\s+\d+|CHAPTER\s+[IVX\d]+|Part\s+[IVX\d]+|Section\s+\d+|\d+(\.\d+)*\s+\S+|[A-Z]\.\s+\S+|[IVX]+\.\s+\S+|推荐语|前言|序言|目录|Preface|Introduction|Contents|致谢|Acknowledgments|摘要|Abstract)\s*$"
    ).unwrap();

    // 统计连续匹配的目录条目（而非单个匹配行），减少正文误判
    let mut toc_like_lines = 0;
    let mut consecutive_toc = 0;
    for line in &lines {
        let trimmed = line.trim();
        if trimmed.len() < 80 && toc_re.is_match(trimmed) {
            consecutive_toc += 1;
            if consecutive_toc >= 2 {
                toc_like_lines += 1;
            }
        } else if !trimmed.is_empty() {
            consecutive_toc = 0;
        }
    }
    let has_toc = toc_like_lines > 5;

    // 评分：标题层级合理（有H1/H2）+ 段落长度适中
    let has_h1 = heading_counts[0] > 0;
    let has_h2 = heading_counts[1] > 0;
    let score = if has_h1 && has_h2 {
        90.0
    } else if has_h2 {
        70.0
    } else {
        40.0
    };
    let score = score - (short_paragraphs as f32 * 0.5).min(20.0);

    StructuralIntegrity {
        heading_counts,
        avg_paragraph_length,
        paragraph_count,
        short_paragraphs,
        long_paragraphs,
        has_toc,
        toc_line_count: toc_like_lines,
        score,
    }
}

// ═══════════════════════════════════════════════════════
//  3. 噪声污染度
// ═══════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct NoisePollution {
    pub repeated_lines: usize,
    pub top_repeated_phrases: Vec<(String, usize)>,
    pub watermark_detected: bool,
    pub watermark_text: String,
    pub special_char_ratio: f32,
    pub page_break_count: usize,
    pub score: f32,
}

/// 判断一行是否可能是代码/配置/表格（应降低重复惩罚）
fn is_likely_code_or_table(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Markdown 表格行
    if trimmed.starts_with('|') && trimmed.ends_with('|') {
        return true;
    }
    // 代码围栏
    if trimmed.starts_with("```") {
        return true;
    }
    // 缩进代码块
    if line.starts_with("    ") || line.starts_with('\t') {
        return true;
    }
    // 高代码字符比例（括号、等号、冒号等）
    const CODE_SYMBOLS: [char; 28] = [
        '{', '}', '[', ']', '(', ')', '=', ':', ';', '+', '-', '*', '/', '|', '&', '#', '<', '>',
        '.', ',', '_', '%', '!', '?', '\'', '"', '`', '~',
    ];
    let code_chars: usize = trimmed.chars().filter(|c| CODE_SYMBOLS.contains(c)).count();
    let ratio = code_chars as f32 / trimmed.len() as f32;
    ratio > 0.3
}

pub fn noise_pollution(text: &str) -> NoisePollution {
    let lines: Vec<&str> = text.lines().collect();

    // 统计重复行（排除代码/表格）
    let mut line_counts: HashMap<String, usize> = HashMap::new();
    let mut code_line_counts: HashMap<String, usize> = HashMap::new();
    for line in &lines {
        let trimmed = line.trim();
        if trimmed.len() < 10 {
            continue;
        }
        if is_likely_code_or_table(line) {
            *code_line_counts.entry(trimmed.to_string()).or_insert(0) += 1;
        } else {
            *line_counts.entry(trimmed.to_string()).or_insert(0) += 1;
        }
    }

    let mut repeated = 0usize;
    let mut top_phrases = Vec::new();
    for (line, count) in &line_counts {
        if *count >= 3 {
            repeated += count - 1;
            top_phrases.push((line.clone(), *count));
        }
    }

    // 代码重复单独统计（降低权重）
    let mut code_repeated = 0usize;
    for count in code_line_counts.values() {
        if *count >= 3 {
            // 代码重复只计1/4权重
            code_repeated += (count - 1) / 4;
            // 代码重复不加入 top_phrases（避免干扰噪声判断）
        }
    }

    top_phrases.sort_by_key(|b| std::cmp::Reverse(b.1));
    top_phrases.truncate(5);

    // 水印检测（通用模式，与preprocess一致）
    let watermark_re = Regex::new(
        r"(?i)(epubw\.[a-z]+|libgen\.[a-z]+|电子书\s*下载|本书由.*整理|仅供学习|版权所有|all rights reserved|scanned by)"
    ).unwrap();
    let watermark_lines: Vec<&str> = lines
        .iter()
        .filter(|l| watermark_re.is_match(l))
        .copied()
        .collect();
    let watermark_detected = !watermark_lines.is_empty();
    let watermark_text = if watermark_detected {
        watermark_lines.first().unwrap().trim().to_string()
    } else {
        String::new()
    };

    // 特殊字符（分页符、控制字符等）
    let special_chars: usize = text
        .chars()
        .filter(|&c| {
            let code = c as u32;
            c == '\x0C' || (code <= 0x08) || (0x0E..=0x1F).contains(&code)
        })
        .count();
    let special_char_ratio = special_chars as f32 / text.len().max(1) as f32;
    let page_break_count = text.matches('\x0C').count();

    // 评分：重复越多、水印越多，分数越低
    // 代码/表格重复权重降低（已除以4），避免技术书代码片段拉低评分
    let total_repeated = repeated + code_repeated;
    let repeat_penalty = (total_repeated as f32 * 0.5).min(25.0);
    let watermark_penalty = if watermark_detected { 20.0 } else { 0.0 };
    let score = (100.0 - repeat_penalty - watermark_penalty).max(0.0);

    NoisePollution {
        repeated_lines: repeated,
        top_repeated_phrases: top_phrases,
        watermark_detected,
        watermark_text,
        special_char_ratio,
        page_break_count,
        score,
    }
}

// ═══════════════════════════════════════════════════════
//  4. 语义连贯性
// ═══════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct SemanticCoherence {
    pub forced_break_count: usize,
    pub broken_urls: usize,
    pub broken_sentences: usize,
    pub avg_line_length: f32,
    pub short_line_ratio: f32,
    pub score: f32,
}

pub fn semantic_coherence(text: &str) -> SemanticCoherence {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return SemanticCoherence {
            forced_break_count: 0,
            broken_urls: 0,
            broken_sentences: 0,
            avg_line_length: 0.0,
            short_line_ratio: 0.0,
            score: 0.0,
        };
    }

    // 强制换行检测：当前行不以标点结尾，下一行以小写字母或中文开头
    let punctuation_end = Regex::new(r"[。，；：！？.!?;:,]$").unwrap();
    let lower_start = Regex::new(r"^[a-z]").unwrap();
    let cjk_start = Regex::new(r"^[一-鿿]").unwrap();

    let mut forced_breaks = 0;
    let mut broken_sentences = 0;

    for i in 0..lines.len().saturating_sub(1) {
        let curr = lines[i].trim();
        let next = lines[i + 1].trim();
        if curr.is_empty() || next.is_empty() {
            continue;
        }

        let curr_ends_with_punct = punctuation_end.is_match(curr);
        let next_starts_lower = lower_start.is_match(next);
        let next_starts_cjk = cjk_start.is_match(next);

        if !curr_ends_with_punct && (next_starts_lower || next_starts_cjk) {
            forced_breaks += 1;
            if next_starts_cjk {
                broken_sentences += 1;
            }
        }
    }

    // URL 断裂检测：http 被换行断开
    let url_start_re = Regex::new(r"https?://$").unwrap();
    let mut broken_urls = 0;
    for i in 0..lines.len().saturating_sub(1) {
        let curr = lines[i].trim();
        if url_start_re.is_match(curr) || curr.ends_with('/') && !curr.contains(' ') {
            // 简化检测：行尾是 / 且下一行继续
            let next = lines[i + 1].trim();
            if !next.is_empty() && !next.starts_with('#') && !next.starts_with("第") {
                broken_urls += 1;
            }
        }
    }

    let non_empty: Vec<&str> = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .copied()
        .collect();
    let total_len: usize = non_empty.iter().map(|l| l.len()).sum();
    let avg_line_length = total_len as f32 / non_empty.len().max(1) as f32;

    let short_lines = non_empty.iter().filter(|l| l.trim().len() < 30).count();
    let short_line_ratio = short_lines as f32 / non_empty.len().max(1) as f32;

    // 评分：强制换行权重降低（对 LLM 理解影响有限）
    let break_penalty = (forced_breaks as f32 * 0.02).min(8.0);
    let url_penalty = (broken_urls as f32 * 2.0).min(15.0);
    let score = (100.0 - break_penalty - url_penalty).max(0.0);

    SemanticCoherence {
        forced_break_count: forced_breaks,
        broken_urls,
        broken_sentences,
        avg_line_length,
        short_line_ratio,
        score,
    }
}

// ═══════════════════════════════════════════════════════
//  5. 内容完整性
// ═══════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct ContentCompleteness {
    pub total_chars: usize,
    pub non_empty_pages_estimate: usize,
    pub blank_ratio: f32,
    pub content_density: f32,
    pub has_references: bool,
    pub reference_count: usize,
    pub score: f32,
}

pub fn content_completeness(text: &str) -> ContentCompleteness {
    let total = text.len();
    let lines: Vec<&str> = text.lines().collect();

    // 估算页数：按平均每页 400-500 字，或按分页符
    let page_breaks = text.matches('\x0C').count();
    let estimated_pages = if page_breaks > 0 {
        page_breaks + 1
    } else {
        total / 450 + 1
    };

    // 空白页估算：连续空行超过一定阈值
    let mut blank_sections = 0;
    let mut empty_streak = 0;
    for line in &lines {
        if line.trim().is_empty() {
            empty_streak += 1;
        } else {
            if empty_streak > 5 {
                blank_sections += 1;
            }
            empty_streak = 0;
        }
    }

    let non_empty_pages = estimated_pages.saturating_sub(blank_sections);
    let blank_ratio = blank_sections as f32 / estimated_pages.max(1) as f32;
    let content_density = total as f32 / estimated_pages.max(1) as f32;

    // 参考文献检测
    let ref_re = Regex::new(r"参考文献|References?|Bibliography|\[\d+\]").unwrap();
    let reference_count = lines.iter().filter(|l| ref_re.is_match(l.trim())).count();
    let has_references = reference_count > 0;

    // 评分
    let density_score = if content_density > 300.0 {
        100.0
    } else if content_density > 100.0 {
        70.0
    } else {
        40.0
    };
    let blank_penalty = (blank_ratio * 30.0).min(20.0);
    let score = (density_score - blank_penalty).max(0.0);

    ContentCompleteness {
        total_chars: total,
        non_empty_pages_estimate: non_empty_pages,
        blank_ratio,
        content_density,
        has_references,
        reference_count,
        score,
    }
}
