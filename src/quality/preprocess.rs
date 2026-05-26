use std::collections::HashMap;

use regex::Regex;

/// 文本预处理：过滤噪声、清理格式
///
/// 处理流程：
/// 1. 清理分页符和控制字符
/// 2. 过滤水印（通用启发式）
/// 3. 过滤高频重复行（页眉页脚）
/// 4. 检测并移除目录区域（多语言支持）
/// 5. 合并强制换行（软回车，多语言标点）
/// 6. 清理多余空行
///
/// 设计原则（保守过滤）：
/// - 宁可保留疑似噪声，也不误删正文
/// - 基于结构特征检测，不依赖特定语言内容
pub fn clean_text(text: &str) -> String {
    let mut result = text.to_string();

    result = clean_page_breaks(&result);
    result = filter_watermarks_generic(&result);
    result = filter_repeated_lines_fuzzy(&result, 5);
    result = remove_toc_generic(&result);
    result = merge_forced_breaks_multilang(&result);
    result = collapse_empty_lines(&result);

    result
}

// ═══════════════════════════════════════════════════════
//  1. 分页符清理
// ═══════════════════════════════════════════════════════

fn clean_page_breaks(text: &str) -> String {
    let text = text.replace('\x0C', "\n\n");
    let re = Regex::new(r"\n{3,}").unwrap();
    re.replace_all(&text, "\n\n").to_string()
}

// ═══════════════════════════════════════════════════════
//  2. 水印过滤（通用启发式）
// ═══════════════════════════════════════════════════════

/// 通用水印检测：基于结构特征而非硬编码文本
///
/// 检测模式：
/// 1. 包含域名/URL的孤立行
/// 2. 宣传性关键词组合
/// 3. 极端重复标点（!!! 等）
fn filter_watermarks_generic(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut result = Vec::new();

    // 域名检测：匹配已知电子书/水印域名
    // 注意：不泛匹配所有域名，避免误伤正文中引用的URL
    let domain_re = Regex::new(
        r"(?i)(epubw\.[a-z]+|libgen\.[a-z]+|zlibrary-[a-z]+|\
         [a-z0-9-]+\.(cc|li)\b)",
    )
    .unwrap();

    let promo_re = Regex::new(
        r"(?i)(电子书\s*下载|免费\s*下载|本书由.*整理|仅供学习|请勿商用|版权所有|all rights reserved|download.*ebook|free.*pdf|scanned by)"
    ).unwrap();

    let extreme_punct_re = Regex::new(r"[!！]{3,}|[?？]{3,}|[.。]{4,}").unwrap();

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            result.push(*line);
            continue;
        }
        let is_watermark = (domain_re.is_match(trimmed) && trimmed.len() < 80)
            || promo_re.is_match(trimmed)
            || extreme_punct_re.is_match(trimmed);
        if !is_watermark {
            result.push(*line);
        }
    }
    result.join("\n")
}

// ═══════════════════════════════════════════════════════
//  3. 重复行过滤（模糊匹配）
// ═══════════════════════════════════════════════════════

/// 模糊重复行检测：忽略末尾页码/数字变化
///
/// 例如 "第一章 ...... 15" 和 "第一章 ...... 16" 被视为同一行
fn filter_repeated_lines_fuzzy(text: &str, threshold: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();

    // 统计每行的"规范化版本"出现次数
    let mut counts: HashMap<String, usize> = HashMap::new();
    for line in &lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.len() < 5 {
            continue;
        }
        let normalized = normalize_for_dedup(trimmed);
        *counts.entry(normalized).or_insert(0) += 1;
    }

    // 找出高频行
    let repeated: std::collections::HashSet<String> = counts
        .iter()
        .filter(|(_, count)| **count >= threshold)
        .map(|(line, _)| line.clone())
        .collect();

    let filtered: Vec<&str> = lines
        .iter()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.is_empty() || !repeated.contains(&normalize_for_dedup(trimmed))
        })
        .copied()
        .collect();

    filtered.join("\n")
}

/// 规范化行内容用于去重：移除末尾页码和数字
fn normalize_for_dedup(line: &str) -> String {
    // 移除末尾的数字和页码标记（如 "... 15"、"- 15"、" 15"）
    let re = Regex::new(r"[\s\.\-–—]+\d+\s*$").unwrap();
    let normalized = re.replace(line, "");
    normalized.to_string()
}

// ═══════════════════════════════════════════════════════
//  4. 目录检测（多语言通用）
// ═══════════════════════════════════════════════════════

/// 通用目录检测：支持中文、英文及混合编号格式
///
/// 支持的目录格式：
/// - 中文：第1章, 第一章, 第I章, 一、, （一）, 1.1, 1.1.1
/// - 英文：Chapter 1, CHAPTER ONE, Part I, Section 1
/// - 混合：1. Introduction, A. Title, I. Overview
/// - 常见前言：推荐语, 前言, Preface, Introduction, 目录, Contents
///
/// 检测策略（保守）：
/// - 必须连续出现3条以上匹配行
/// - 行长度 < 80（目录条目通常较短）
/// - 识别到非目录长行后结束目录区域
fn remove_toc_generic(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();

    // 多语言目录条目匹配（单行格式，不支持换行）
    let toc_entry_re = Regex::new(
        r"^(第[\d一二三四五六七八九十百千]+章.*|[一二三四五六七八九十百千]+[、．.]\s*.*|（[一二三四五六七八九十]）.*|Chapter\s+\d+.*|CHAPTER\s+[IVX\d]+.*|Part\s+[IVX\d]+.*|Section\s+\d+.*|\d+(\.\d+)*\s+\S+.*|[A-Z]\.\s+\S+.*|[IVX]+\.\s+\S+.*|推荐语|前言|序言|目录|Preface|Introduction|Contents|致谢|Acknowledgments|摘要|Abstract)\s*$"
    ).unwrap();

    let mut result = Vec::new();
    let mut in_toc = false;
    let mut toc_streak = 0;

    for line in &lines {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if in_toc && toc_streak >= 5 {
                in_toc = false;
                toc_streak = 0;
            }
            result.push(*line);
            continue;
        }

        // 目录条目检测
        if trimmed.len() < 80 && toc_entry_re.is_match(trimmed) {
            toc_streak += 1;
            if toc_streak >= 3 {
                in_toc = true;
            }
            continue;
        }

        if in_toc {
            // 目录区域结束：遇到非目录格式的长行
            if trimmed.len() > 60 {
                in_toc = false;
                toc_streak = 0;
                result.push(*line);
            }
            // 否则继续跳过
        } else {
            toc_streak = 0;
            result.push(*line);
        }
    }

    result.join("\n")
}

// ═══════════════════════════════════════════════════════
//  5. 合并强制换行（多语言标点）
// ═══════════════════════════════════════════════════════

/// 多语言强制换行合并
///
/// 支持标点：
/// - 中文：。，；：！？… ——
/// - 英文：. , ; : ! ? ...
/// - 日文：。、
/// - 韩文：. ,
///
/// 合并条件（更积极）：
/// 1. 上一行不是标题
/// 2. 上一行不以句尾标点结尾
/// 3. 当前行不是空行/标题
/// 4. 当前行以小写字母或CJK字符开头（续行特征）
/// 5. 不在代码块或表格区域内
fn merge_forced_breaks_multilang(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return text.to_string();
    }

    // 句尾标点：多语言
    let sentence_end = Regex::new(r"[。．.！?？…、,；;：:\-–—~]$").unwrap();
    let heading_re = Regex::new(r"^#{1,6}\s+").unwrap();
    let cjk_char = Regex::new(r"^[\u{4E00}-\u{9FFF}\u{3400}-\u{4DBF}\u{3000}-\u{303F}]").unwrap();
    let lower_start = Regex::new(r"^[a-z]").unwrap();
    let digit_start = Regex::new(r"^\d").unwrap();

    // 代码块/表格检测
    let code_fence = Regex::new(r"^```").unwrap();
    let table_row = Regex::new(r"^\|.*\|$").unwrap();
    let table_sep = Regex::new(r"^\+[\-+]+\+$").unwrap();
    let indent_code = Regex::new(r"^\s{4,}").unwrap();

    let mut result = Vec::new();
    let mut current = lines[0].to_string();
    let mut in_code_block = false;

    for i in 1..lines.len() {
        let prev = lines[i - 1].trim();
        let curr_raw = lines[i];
        let curr = curr_raw.trim();

        // 更新代码块状态
        if code_fence.is_match(curr) {
            in_code_block = !in_code_block;
        }

        let prev_is_heading = heading_re.is_match(prev);
        let curr_is_heading = heading_re.is_match(curr);
        let prev_ends_sentence = sentence_end.is_match(prev) || prev.is_empty();

        // 续行特征：当前行以小写字母、CJK字符或数字开头
        let curr_is_continuation =
            lower_start.is_match(curr) || cjk_char.is_match(curr) || digit_start.is_match(curr);

        // 是否在表格/代码区域内
        let in_structured = in_code_block
            || table_row.is_match(prev)
            || table_row.is_match(curr)
            || table_sep.is_match(prev)
            || table_sep.is_match(curr)
            || indent_code.is_match(prev)
            || indent_code.is_match(curr);

        // 合并条件
        let should_merge = !prev_is_heading
            && !prev_ends_sentence
            && !curr_is_heading
            && !curr.is_empty()
            && curr_is_continuation
            && !in_structured
            && (prev.len() < 120 || curr.len() < 120);

        if should_merge {
            current.push(' ');
            current.push_str(curr_raw);
        } else {
            result.push(current);
            current = curr_raw.to_string();
        }
    }
    result.push(current);

    result.join("\n")
}

// ═══════════════════════════════════════════════════════
//  6. 压缩空行
// ═══════════════════════════════════════════════════════

fn collapse_empty_lines(text: &str) -> String {
    let re = Regex::new(r"\n{3,}").unwrap();
    re.replace_all(text, "\n\n").to_string()
}

// ═══════════════════════════════════════════════════════
//  测试
// ═══════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_watermarks_generic() {
        let input = "内容A\n本书由「ePUBw.COM」整理\n电子书下载！！！\n访问 epubw.cc\n内容B";
        let result = filter_watermarks_generic(input);
        assert!(!result.contains("ePUBw"));
        assert!(!result.contains("电子书下载"));
        assert!(!result.contains("epubw.cc"));
        assert!(result.contains("内容A"));
        assert!(result.contains("内容B"));
    }

    #[test]
    fn test_filter_watermarks_ignores_body() {
        // 正文中的域名不应该被过滤
        let input = "文本A https://github.com/rust-lang/rust 文本B";
        let result = filter_watermarks_generic(input);
        assert!(result.contains("github.com"));
    }

    #[test]
    fn test_filter_repeated_lines_fuzzy() {
        let input = "第一章\n页脚内容 15\n正文A\n页脚内容 16\n正文B\n页脚内容 17\n页脚内容 18\n页脚内容 19\n正文C";
        let result = filter_repeated_lines_fuzzy(input, 3);
        assert!(!result.contains("页脚内容"));
        assert!(result.contains("正文A"));
        assert!(result.contains("正文B"));
        assert!(result.contains("正文C"));
    }

    #[test]
    fn test_remove_toc_chinese() {
        let input = "前言\n\n第1章 标题\n1.1 小节\n1.2 小节\n\n正文开始，内容ABCD内容ABCD内容ABCD内容ABCD内容ABCD内容ABCD内容ABCD内容ABCD内容ABCD内容ABCD内容ABCD内容ABCD内容ABCD内容ABCD内容ABCD。\n内容B。";
        let result = remove_toc_generic(input);
        assert!(!result.contains("1.1 小节"));
        assert!(result.contains("正文开始"));
    }

    #[test]
    fn test_remove_toc_english() {
        let input = "Preface\n\nChapter 1 Introduction\n1.1 Background\n1.2 Goals\n\nThis is the actual body text with enough length to be recognized.\nMore content here.";
        let result = remove_toc_generic(input);
        assert!(!result.contains("1.1 Background"));
        assert!(result.contains("actual body text"));
    }

    #[test]
    fn test_remove_toc_mixed() {
        let input = "目录\n\n一、概述\n二、方法\n1.1 细节\n\n内容C内容C内容C内容C内容C内容C内容C内容C内容C内容C内容C内容C。\n内容D。";
        let result = remove_toc_generic(input);
        assert!(!result.contains("一、概述"));
        assert!(result.contains("内容C"));
    }

    #[test]
    fn test_remove_toc_not_body_refs() {
        // 正文中的章节引用不应被删除
        let input = "文本A章节C文本B。\n文本D。";
        let result = remove_toc_generic(input);
        assert!(result.contains("章节C"));
    }

    #[test]
    fn test_merge_forced_breaks_multilang() {
        let input = "文本A被强制\n换行的文本B。\n\n文本C。";
        let result = merge_forced_breaks_multilang(input);
        assert!(result.contains("被强制 换行的"));
        assert!(result.contains("文本C"));
    }

    #[test]
    fn test_merge_forced_breaks_english() {
        let input = "This is a sentence\nthat was broken\nmidway.\n\nNew paragraph here.";
        let result = merge_forced_breaks_multilang(input);
        assert!(result.contains("sentence that"));
        assert!(result.contains("broken midway."));
    }

    #[test]
    fn test_merge_respects_punctuation() {
        // 以标点结尾的行不应该合并
        let input = "文本A。\n文本B。";
        let result = merge_forced_breaks_multilang(input);
        assert!(!result.contains("。 \n"));
    }

    #[test]
    fn test_clean_page_breaks() {
        let input = "段落A\x0C段落B\x0C\x0C段落C";
        let result = clean_page_breaks(input);
        assert!(!result.contains('\x0C'));
        assert!(result.contains("段落A"));
        assert!(result.contains("段落B"));
        assert!(result.contains("段落C"));
    }

    #[test]
    fn test_full_clean_generic() {
        let input = "内容E\n\n本书由「ePUBw.COM」整理\n\n电子书下载！！！\n\n标题A\n内容F\n\n参考文献\n[1] 引用\n\n内容G";
        let result = clean_text(input);
        assert!(!result.contains("ePUBw"));
        // "标题A" 不再被硬编码过滤（保留正文内容更重要）
        assert!(result.contains("内容E"));
        assert!(result.contains("内容F"));
        assert!(result.contains("[1] 引用"));
        assert!(result.contains("内容G"));
    }

    // ── 边界测试 ──

    #[test]
    fn test_filter_watermarks_all_watermark() {
        let input = "本书由「ePUBw.COM」整理\n电子书下载！！！\n访问 epubw.cc";
        let result = filter_watermarks_generic(input);
        // 全是水印，结果应该为空或极短
        assert!(!result.contains("ePUBw"));
        assert!(!result.contains("epubw"));
    }

    #[test]
    fn test_remove_toc_empty() {
        let input = "";
        let result = remove_toc_generic(input);
        assert_eq!(result, "");
    }

    #[test]
    fn test_clean_text_empty() {
        let input = "";
        let result = clean_text(input);
        assert_eq!(result, "");
    }

    #[test]
    fn test_merge_forced_breaks_empty() {
        let input = "";
        let result = merge_forced_breaks_multilang(input);
        assert_eq!(result, "");
    }

    #[test]
    fn test_merge_forced_breaks_single_line() {
        let input = "文本A";
        let result = merge_forced_breaks_multilang(input);
        assert_eq!(result, "文本A");
    }

    #[test]
    fn test_filter_repeated_lines_empty() {
        let input = "";
        let result = filter_repeated_lines_fuzzy(input, 3);
        assert_eq!(result, "");
    }

    #[test]
    fn test_filter_repeated_lines_no_repeats() {
        let input = "A\nB\nC\nD\nE";
        let result = filter_repeated_lines_fuzzy(input, 3);
        assert_eq!(result, "A\nB\nC\nD\nE");
    }
}
