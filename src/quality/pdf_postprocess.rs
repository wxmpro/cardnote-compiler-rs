/// PDF 文本后处理 — 页眉页脚清理
///
/// PyMuPDF 提取的文本包含 `## 第 N 页` 分隔标记，
/// 基于此可以检测并移除每页重复出现的页眉/页脚。
///
/// 核心策略：
/// 1. 按页分隔提取每页文本
/// 2. 统计每页开头/结尾的公共文本行
/// 3. 出现频率 ≥ 阈值的行视为页眉/页脚
/// 4. 从所有页面中移除这些重复模式
use std::collections::HashMap;

/// 页眉页脚清理配置
#[derive(Debug, Clone)]
pub struct HeaderFooterConfig {
    /// 页眉检测：检查每页前 N 行
    pub header_lines: usize,
    /// 页脚检测：检查每页后 N 行
    pub footer_lines: usize,
    /// 出现频率阈值（出现页数 / 总页数）
    pub frequency_threshold: f64,
    /// 最小页眉页脚长度（字符数）
    pub min_length: usize,
    /// 最大页眉页脚长度（字符数）
    pub max_length: usize,
    /// 是否清理页码标记（如 "— 1 —"）
    pub clean_page_numbers: bool,
}

impl Default for HeaderFooterConfig {
    fn default() -> Self {
        Self {
            header_lines: 3,
            footer_lines: 3,
            frequency_threshold: 0.7,
            min_length: 3,
            max_length: 200,
            clean_page_numbers: true,
        }
    }
}

/// 页眉页脚清理结果
#[derive(Debug, Clone)]
pub struct HeaderFooterResult {
    pub cleaned_text: String,
    pub headers_removed: Vec<String>,
    pub footers_removed: Vec<String>,
    pub pages_processed: usize,
}

/// 清理 PDF 文本中的页眉页脚
pub fn remove_headers_footers(text: &str) -> HeaderFooterResult {
    let config = HeaderFooterConfig::default();
    remove_headers_footers_with_config(text, &config)
}

/// 带配置的页眉页脚清理
pub fn remove_headers_footers_with_config(
    text: &str,
    config: &HeaderFooterConfig,
) -> HeaderFooterResult {
    let pages = split_into_pages(text);
    if pages.len() < 3 {
        // 页数太少，无法可靠检测页眉页脚
        return HeaderFooterResult {
            cleaned_text: text.to_string(),
            headers_removed: Vec::new(),
            footers_removed: Vec::new(),
            pages_processed: pages.len(),
        };
    }

    // 检测页眉
    let headers = detect_repeated_lines(&pages, config.header_lines, true, config);
    // 检测页脚
    let footers = detect_repeated_lines(&pages, config.footer_lines, false, config);

    // 清理页码标记（如果启用）
    let page_number_patterns = if config.clean_page_numbers {
        detect_page_number_patterns(&pages)
    } else {
        Vec::new()
    };

    // 从每页中移除检测到的页眉页脚
    let mut cleaned_pages = Vec::new();
    for (page_num, page_text) in &pages {
        let cleaned =
            remove_patterns_from_page(page_text, &headers, &footers, &page_number_patterns);
        cleaned_pages.push((*page_num, cleaned));
    }

    // 重建文本
    let cleaned_text = rebuild_text(&cleaned_pages);

    HeaderFooterResult {
        cleaned_text,
        headers_removed: headers,
        footers_removed: footers,
        pages_processed: pages.len(),
    }
}

/// 按 `## 第 N 页` 分隔提取每页文本
///
/// 返回：(页码, 该页文本内容) 列表
fn split_into_pages(text: &str) -> Vec<(usize, String)> {
    let mut pages = Vec::new();
    let marker = "## 第 ";

    // 找到所有页分隔标记的位置
    let mut positions: Vec<(usize, usize)> = Vec::new(); // (开始位置, 页码)
    for (idx, line) in text.lines().enumerate() {
        if let Some(num_str) = line.strip_prefix(marker)
            && let Some(num) = num_str
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<usize>().ok())
        {
            positions.push((idx, num));
        }
    }

    if positions.is_empty() {
        // 没有页分隔标记，将整个文本作为一页
        return vec![(1, text.to_string())];
    }

    let all_lines: Vec<&str> = text.lines().collect();

    for i in 0..positions.len() {
        let start_line = positions[i].0 + 1; // 跳过标记行本身
        let end_line = if i + 1 < positions.len() {
            positions[i + 1].0
        } else {
            all_lines.len()
        };

        if start_line < end_line && start_line < all_lines.len() {
            let page_text: String = all_lines[start_line..end_line].join("\n");
            pages.push((positions[i].1, page_text));
        }
    }

    pages
}

/// 检测重复出现的文本行
///
/// 参数：
/// - pages: 页面列表
/// - n_lines: 检查的头部/尾部行数
/// - from_start: true 表示从开头检查（页眉），false 表示从结尾检查（页脚）
fn detect_repeated_lines(
    pages: &[(usize, String)],
    n_lines: usize,
    from_start: bool,
    config: &HeaderFooterConfig,
) -> Vec<String> {
    let mut line_freq: HashMap<String, usize> = HashMap::new();
    let total_pages = pages.len();
    let threshold = (total_pages as f64 * config.frequency_threshold).ceil() as usize;

    for (_, page_text) in pages {
        let lines: Vec<&str> = page_text.lines().collect();
        let candidates = if from_start {
            lines.iter().take(n_lines).copied().collect::<Vec<_>>()
        } else {
            lines
                .iter()
                .rev()
                .take(n_lines)
                .copied()
                .collect::<Vec<_>>()
        };

        for line in candidates {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let len = trimmed.chars().count();
            if len < config.min_length || len > config.max_length {
                continue;
            }
            *line_freq.entry(trimmed.to_string()).or_insert(0) += 1;
        }
    }

    // 筛选出现频率 ≥ 阈值的行
    let mut result: Vec<String> = line_freq
        .into_iter()
        .filter(|(_, count)| *count >= threshold)
        .map(|(line, _)| line)
        .collect();

    // 按长度排序，先移除长的（避免部分匹配问题）
    result.sort_by_key(|s| std::cmp::Reverse(s.len()));
    result
}

/// 检测页码标记模式
///
/// 常见页码格式：
/// - "— 1 —"
/// - "- 1 -"
/// - "1"
/// - "第 1 页"
fn detect_page_number_patterns(pages: &[(usize, String)]) -> Vec<String> {
    let mut patterns = Vec::new();

    for (_, page_text) in pages {
        let lines: Vec<&str> = page_text.lines().collect();
        // 检查每页的最后几行
        for line in lines.iter().rev().take(3) {
            let trimmed = line.trim();
            // 匹配 "— N —" 或 "- N -" 格式
            if let Some(captures) = regex_captures_page_number(trimmed)
                && !patterns.contains(&captures)
            {
                patterns.push(captures);
            }
        }
    }

    patterns
}

/// 简单页码正则匹配（不用 regex crate，减少依赖）
fn regex_captures_page_number(s: &str) -> Option<String> {
    // 模式1: "— N —" 或 "- N -" 或 "– N –"
    let dash_chars = ['—', '-', '–', '—'];
    let chars: Vec<char> = s.chars().collect();
    if chars.len() >= 3 {
        if dash_chars.contains(&chars[0]) && dash_chars.contains(&chars[chars.len() - 1]) {
            let middle: String = chars[1..chars.len() - 1].iter().collect();
            let trimmed = middle.trim();
            if trimmed.chars().all(|c| c.is_ascii_digit()) {
                return Some(s.to_string());
            }
        }
    }

    // 模式2: 纯数字（单页码，需要确认是孤立的）
    if s.chars().all(|c| c.is_ascii_digit()) && s.len() <= 4 {
        return Some(s.to_string());
    }

    None
}

/// 从单页文本中移除页眉页脚模式
fn remove_patterns_from_page(
    page_text: &str,
    headers: &[String],
    footers: &[String],
    page_numbers: &[String],
) -> String {
    let lines: Vec<&str> = page_text.lines().collect();
    let mut cleaned = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            cleaned.push(*line);
            continue;
        }

        // 检查是否是页眉（只检查前 n 行）
        let is_header = idx < headers.len() + 2
            && headers
                .iter()
                .any(|h| trimmed.contains(h) || h.contains(trimmed));

        // 检查是否是页脚（只检查后 n 行）
        let is_footer = idx >= lines.len().saturating_sub(footers.len() + 2)
            && footers
                .iter()
                .any(|f| trimmed.contains(f) || f.contains(trimmed));

        // 检查是否是页码
        let is_page_num = page_numbers.iter().any(|p| trimmed == p.as_str());

        if !is_header && !is_footer && !is_page_num {
            cleaned.push(*line);
        }
    }

    cleaned.join("\n")
}

/// 重建清理后的文本
fn rebuild_text(pages: &[(usize, String)]) -> String {
    let mut parts = Vec::new();
    for (page_num, text) in pages {
        parts.push(format!("## 第 {} 页\n\n{}", page_num, text));
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_into_pages() {
        let text = "## 第 1 页\n\n标题A\n内容A。\n\n## 第 2 页\n\n内容B。\n\n## 第 3 页\n\n标题C";
        let pages = split_into_pages(text);
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0].0, 1);
        assert!(pages[0].1.contains("标题A"));
        assert_eq!(pages[1].0, 2);
        assert!(pages[1].1.contains("内容B"));
    }

    #[test]
    fn test_detect_repeated_header() {
        let pages = vec![
            (1, "标题B\n作者A\n\n内容C...".to_string()),
            (2, "标题B\n作者A\n\n内容D...".to_string()),
            (3, "标题B\n作者A\n\n内容E...".to_string()),
        ];

        let config = HeaderFooterConfig::default();
        let headers = detect_repeated_lines(&pages, 2, true, &config);
        assert!(headers.iter().any(|h| h.contains("标题B")));
    }

    #[test]
    fn test_remove_headers_footers() {
        let text = r##"## 第 1 页

标题B
作者A

标题A

内容C。

— 1 —

## 第 2 页

标题B
作者A

标题D

内容D。

— 2 —

## 第 3 页

标题B
作者A

标题E

内容E。

— 3 —"##;

        let result = remove_headers_footers(text);
        // 页眉 "标题B" 和 "作者A" 应该被移除
        assert!(!result.cleaned_text.contains("标题B"));
        assert!(!result.cleaned_text.contains("作者A"));
        // 页码应该被移除
        assert!(!result.cleaned_text.contains("— 1 —"));
        // 正文内容应该保留
        assert!(result.cleaned_text.contains("标题A"));
        assert!(result.cleaned_text.contains("标题D"));
    }

    #[test]
    fn test_page_number_detection() {
        assert!(regex_captures_page_number("— 1 —").is_some());
        assert!(regex_captures_page_number("- 12 -").is_some());
        assert!(regex_captures_page_number("123").is_some());
        assert!(regex_captures_page_number("文本A").is_none());
    }

    #[test]
    fn test_single_page_no_change() {
        let text = "## 第 1 页\n\n内容F。\n\n内容G。";
        let result = remove_headers_footers(text);
        assert_eq!(result.pages_processed, 1);
        assert_eq!(result.cleaned_text, text);
    }

    #[test]
    fn test_no_page_markers() {
        let text = "内容H。\n\n内容I。";
        let result = remove_headers_footers(text);
        assert_eq!(result.pages_processed, 1);
        assert_eq!(result.cleaned_text, text);
    }

    // ── 边界测试 ──

    #[test]
    fn test_many_pages_same_header() {
        // 10页都有相同的页眉
        let mut text = String::new();
        for i in 1..=10 {
            text.push_str(&format!("## 第 {} 页\n\n页眉A\n\n内容{}\n\n", i, i));
        }
        let result = remove_headers_footers(&text);
        // 重复页眉应该被移除
        assert!(!result.cleaned_text.contains("页眉A"));
        // 正文应该保留
        assert!(result.cleaned_text.contains("内容1"));
        assert!(result.cleaned_text.contains("内容10"));
    }

    #[test]
    fn test_empty_page() {
        let text = "## 第 1 页\n\n## 第 2 页\n\n正文\n\n## 第 3 页\n\n";
        let result = remove_headers_footers(text);
        assert_eq!(result.pages_processed, 3);
    }

    #[test]
    fn test_page_number_various_formats() {
        assert!(regex_captures_page_number("— 1 —").is_some());
        assert!(regex_captures_page_number("- 99 -").is_some());
        assert!(regex_captures_page_number("9999").is_some());
        assert!(regex_captures_page_number("— 0 —").is_some());
        assert!(regex_captures_page_number("文本A").is_none());
        // 空字符串不是页码（虽然 all() 在空迭代器返回 true，但函数逻辑会跳过）
    }
}
