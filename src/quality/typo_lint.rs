/// 排版问题类型
#[derive(Debug, Clone, PartialEq)]
pub enum TypoIssue {
    /// 中英文之间缺少空格
    MissingSpaceBetweenCjkAndLatin { position: usize, snippet: String },
    /// 中文与数字之间缺少空格（可选，根据上下文）
    MissingSpaceBetweenCjkAndNumber { position: usize, snippet: String },
    /// 中文内容使用了英文标点
    LatinPunctuationInCjk {
        position: usize,
        char: char,
        expected: char,
    },
    /// 连续重复标点
    RepeatedPunctuation {
        position: usize,
        char: char,
        count: usize,
    },
    /// 多余连续空格
    ExcessiveSpaces { position: usize, count: usize },
    /// 中英文括号混用
    MixedBrackets {
        position: usize,
        found: char,
        expected: char,
    },
    /// 中英文引号混用
    MixedQuotes { position: usize, found: char },
    /// 行首/行尾空格
    LeadingTrailingSpace { position: usize, is_leading: bool },
}

impl std::fmt::Display for TypoIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypoIssue::MissingSpaceBetweenCjkAndLatin { position, snippet } => {
                write!(f, "位置{}: 中英文之间缺少空格: '{}'", position, snippet)
            }
            TypoIssue::MissingSpaceBetweenCjkAndNumber { position, snippet } => {
                write!(f, "位置{}: 中文与数字之间缺少空格: '{}'", position, snippet)
            }
            TypoIssue::LatinPunctuationInCjk {
                position,
                char,
                expected,
            } => {
                write!(
                    f,
                    "位置{}: 应使用中文标点 '{}' 而非 '{}'",
                    position, expected, char
                )
            }
            TypoIssue::RepeatedPunctuation {
                position,
                char,
                count,
            } => {
                write!(f, "位置{}: 连续{}个 '{}'", position, count, char)
            }
            TypoIssue::ExcessiveSpaces { position, count } => {
                write!(f, "位置{}: 连续{}个空格", position, count)
            }
            TypoIssue::MixedBrackets {
                position,
                found,
                expected,
            } => {
                write!(
                    f,
                    "位置{}: 应使用中文括号 '{}' 而非 '{}'",
                    position, expected, found
                )
            }
            TypoIssue::MixedQuotes { position, found } => {
                write!(f, "位置{}: 混用引号 '{}'", position, found)
            }
            TypoIssue::LeadingTrailingSpace {
                position,
                is_leading,
            } => {
                let where_ = if *is_leading { "行首" } else { "行尾" };
                write!(f, "位置{}: {}空格", position, where_)
            }
        }
    }
}

/// 排版lint配置
#[derive(Debug, Clone)]
pub struct TypoLintConfig {
    /// 是否检查中英文空格
    pub check_cjk_latin_space: bool,
    /// 是否检查中文数字空格（较宽松，默认关闭）
    pub check_cjk_number_space: bool,
    /// 是否检查英文标点
    pub check_latin_punctuation: bool,
    /// 是否检查重复标点
    pub check_repeated_punctuation: bool,
    /// 是否检查多余空格
    pub check_excessive_spaces: bool,
    /// 是否检查括号混用
    pub check_mixed_brackets: bool,
    /// 是否检查引号混用
    pub check_mixed_quotes: bool,
    /// 是否检查行首行尾空格
    pub check_leading_trailing_space: bool,
    /// 最大允许连续空格数
    pub max_consecutive_spaces: usize,
    /// 最大允许连续相同标点数
    pub max_consecutive_punctuation: usize,
}

impl Default for TypoLintConfig {
    fn default() -> Self {
        Self {
            check_cjk_latin_space: true,
            check_cjk_number_space: false,
            check_latin_punctuation: true,
            check_repeated_punctuation: true,
            check_excessive_spaces: true,
            check_mixed_brackets: true,
            check_mixed_quotes: true,
            check_leading_trailing_space: true,
            max_consecutive_spaces: 2,
            max_consecutive_punctuation: 1,
        }
    }
}

/// 排版检查结果
#[derive(Debug, Clone)]
pub struct TypoLintResult {
    pub issues: Vec<TypoIssue>,
    pub fixed_text: String,
    pub issue_count: usize,
}

/// 检测文本中的排版问题并自动修复
pub fn typo_lint(text: &str) -> TypoLintResult {
    let config = TypoLintConfig::default();
    typo_lint_with_config(text, &config)
}

/// 带配置的排版检查
pub fn typo_lint_with_config(text: &str, config: &TypoLintConfig) -> TypoLintResult {
    let mut issues = Vec::new();
    let mut fixed = text.to_string();

    // 1. 检查行首行尾空格（最先检查，避免后续修复干扰位置）
    if config.check_leading_trailing_space {
        check_leading_trailing_spaces(text, &mut issues);
    }

    // 2. 检查多余连续空格
    if config.check_excessive_spaces {
        fixed = fix_excessive_spaces(&fixed, config.max_consecutive_spaces, &mut issues);
    }

    // 3. 检查连续重复标点
    if config.check_repeated_punctuation {
        fixed = fix_repeated_punctuation(&fixed, config.max_consecutive_punctuation, &mut issues);
    }

    // 4. 检查中英文空格
    if config.check_cjk_latin_space {
        fixed = fix_cjk_latin_spacing(&fixed, &mut issues);
    }

    // 5. 检查中文数字空格
    if config.check_cjk_number_space {
        fixed = fix_cjk_number_spacing(&fixed, &mut issues);
    }

    // 6. 检查英文标点
    if config.check_latin_punctuation {
        fixed = fix_latin_punctuation(&fixed, &mut issues);
    }

    // 7. 检查括号混用
    if config.check_mixed_brackets {
        fixed = fix_mixed_brackets(&fixed, &mut issues);
    }

    // 8. 检查引号混用
    if config.check_mixed_quotes {
        fixed = fix_mixed_quotes(&fixed, &mut issues);
    }

    let issue_count = issues.len();
    TypoLintResult {
        issues,
        fixed_text: fixed,
        issue_count,
    }
}

// ── 检测与修复函数 ──────────────────────────────────────────────

fn check_leading_trailing_spaces(text: &str, issues: &mut Vec<TypoIssue>) {
    for (line_idx, line) in text.lines().enumerate() {
        if line.starts_with(' ') {
            issues.push(TypoIssue::LeadingTrailingSpace {
                position: line_idx,
                is_leading: true,
            });
        }
        if line.ends_with(' ') && !line.is_empty() {
            issues.push(TypoIssue::LeadingTrailingSpace {
                position: line_idx,
                is_leading: false,
            });
        }
    }
}

fn fix_excessive_spaces(text: &str, max: usize, _issues: &mut Vec<TypoIssue>) -> String {
    // 将连续超过 max 个空格缩减为 max 个
    // 保留 Markdown 表格格式（表格行以 | 开头）
    let mut result = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('|') || trimmed.starts_with("|-") {
            // 保留表格行原样
            result.push_str(line);
        } else {
            // 缩减连续空格
            let chars = line.chars().peekable();
            let mut space_count = 0;
            for c in chars {
                if c == ' ' {
                    space_count += 1;
                    if space_count <= max {
                        result.push(c);
                    }
                } else {
                    space_count = 0;
                    result.push(c);
                }
            }
        }
        result.push('\n');
    }
    // 移除最后的换行（如果原文不以换行结尾）
    if !text.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }
    result
}

fn fix_repeated_punctuation(text: &str, max: usize, _issues: &mut Vec<TypoIssue>) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last_char: Option<char> = None;
    let mut repeat_count = 0;

    for c in text.chars() {
        if is_repeatable_punctuation(c) && Some(c) == last_char {
            repeat_count += 1;
            if repeat_count <= max {
                result.push(c);
            }
            // 超过 max 的重复标点被丢弃
        } else {
            repeat_count = 1;
            result.push(c);
            last_char = Some(c);
        }
    }
    result
}

fn is_repeatable_punctuation(c: char) -> bool {
    matches!(
        c,
        '。' | '，' | '、' | '；' | '：' | '！' | '？' | '.' | ',' | ';' | ':' | '!'
    )
}

/// CJK Unicode 范围判断
fn is_cjk(c: char) -> bool {
    matches!(
        c,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}' // CJK Extension A
        | '\u{3000}'..='\u{303F}' // CJK Symbols and Punctuation
        | '\u{FF00}'..='\u{FFEF}' // Fullwidth forms
        | '\u{2E80}'..='\u{2EFF}' // CJK Radicals Supplement
        | '\u{20000}'..='\u{2A6DF}' // CJK Extension B
    )
}

fn is_latin(c: char) -> bool {
    c.is_ascii_alphabetic()
}

fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

/// 检查一个字符是否是中文标点
fn is_cjk_punctuation(c: char) -> bool {
    matches!(
        c,
        '。' | '，'
            | '、'
            | '；'
            | '：'
            | '！'
            | '？'
            | '\u{201C}'
            | '\u{201D}'
            | '\u{2018}'
            | '\u{2019}'
            | '（'
            | '）'
            | '【'
            | '】'
            | '《'
            | '》'
            | '〈'
            | '〉'
            | '「'
            | '」'
            | '『'
            | '』'
            | '—'
            | '…'
            | '·'
            | '～'
    )
}

/// 修复中英文之间缺少空格
fn fix_cjk_latin_spacing(text: &str, issues: &mut Vec<TypoIssue>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len() + chars.len() / 10);
    let mut pos = 0;

    while pos < chars.len() {
        let c = chars[pos];
        result.push(c);

        // 检查 CJK 后面紧跟 Latin
        if pos + 1 < chars.len() {
            let next = chars[pos + 1];
            if (is_cjk(c) && is_latin(next)) || (is_latin(c) && is_cjk(next)) {
                // 检查中间是否已有空格或换行或标点
                if next != ' '
                    && next != '\n'
                    && !is_cjk_punctuation(next)
                    && !is_cjk_punctuation(c)
                {
                    // 豁免：URL、代码片段、Markdown 链接语法
                    if !is_in_url_or_code(&chars, pos) {
                        let snippet_start = pos.saturating_sub(5);
                        let snippet_end = (pos + 7).min(chars.len());
                        let snippet: String = chars[snippet_start..snippet_end].iter().collect();
                        issues.push(TypoIssue::MissingSpaceBetweenCjkAndLatin {
                            position: pos,
                            snippet,
                        });
                        result.push(' ');
                    }
                }
            }
        }
        pos += 1;
    }
    result
}

/// 豁免检测：是否在 URL、代码块、Markdown 链接中
fn is_in_url_or_code(chars: &[char], pos: usize) -> bool {
    // 简单启发式：如果在代码块(```)内，或URL(http://, https://)内
    // 检查前后是否有代码块标记
    let text_before: String = chars[..=pos.min(chars.len() - 1)].iter().collect();
    let code_fence_count = text_before.matches("```").count();
    if code_fence_count % 2 == 1 {
        return true; // 在代码块内
    }

    // 检查是否是 URL
    let window_start = pos.saturating_sub(10);
    let window: String = chars[window_start..=pos].iter().collect();
    if window.contains("http://") || window.contains("https://") || window.contains("ftp://") {
        return true;
    }

    // 检查是否是 Markdown 链接语法 [](...)
    // 简单检查：前面是否有 ](
    if pos >= 2 {
        let prev2: String = chars[pos.saturating_sub(2)..=pos].iter().collect();
        if prev2.ends_with("](") {
            return true;
        }
    }

    false
}

/// 修复中文与数字之间缺少空格
fn fix_cjk_number_spacing(text: &str, issues: &mut Vec<TypoIssue>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len() + chars.len() / 10);
    let mut pos = 0;

    while pos < chars.len() {
        let c = chars[pos];
        result.push(c);

        if pos + 1 < chars.len() {
            let next = chars[pos + 1];
            if ((is_cjk(c) && is_digit(next)) || (is_digit(c) && is_cjk(next)))
                && next != ' '
                && next != '\n'
                && !is_cjk_punctuation(next)
                && !is_cjk_punctuation(c)
                && !is_in_url_or_code(&chars, pos)
            {
                let snippet_start = pos.saturating_sub(3);
                let snippet_end = (pos + 5).min(chars.len());
                let snippet: String = chars[snippet_start..snippet_end].iter().collect();
                issues.push(TypoIssue::MissingSpaceBetweenCjkAndNumber {
                    position: pos,
                    snippet,
                });
                result.push(' ');
            }
        }
        pos += 1;
    }
    result
}

/// 英文标点 → 中文标点映射
fn latin_to_cjk_punctuation(c: char) -> Option<char> {
    match c {
        ',' => Some('，'),
        '.' => Some('。'),
        ';' => Some('；'),
        ':' => Some('：'),
        '!' => Some('！'),
        '?' => Some('？'),
        '(' => Some('（'),
        ')' => Some('）'),
        '[' => Some('【'),
        ']' => Some('】'),
        '<' => Some('《'),
        '>' => Some('》'),
        // 引号在 fix_mixed_quotes 中单独处理（需要区分左右）
        _ => None,
    }
}

/// 修复中文内容中的英文标点
fn fix_latin_punctuation(text: &str, issues: &mut Vec<TypoIssue>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len());

    for (pos, &c) in chars.iter().enumerate() {
        if let Some(cjk_punct) = latin_to_cjk_punctuation(c) {
            // 检查上下文：前后是否有 CJK 字符
            let has_cjk_before = pos > 0 && is_cjk(chars[pos - 1]);
            let has_cjk_after = pos + 1 < chars.len() && is_cjk(chars[pos + 1]);

            // 如果在 CJK 环境中，替换为中文标点
            if has_cjk_before || has_cjk_after {
                // 豁免：代码块内、URL内、Markdown表格内
                if !is_in_url_or_code(&chars, pos) && !is_in_table(&chars, pos) {
                    issues.push(TypoIssue::LatinPunctuationInCjk {
                        position: pos,
                        char: c,
                        expected: cjk_punct,
                    });
                    result.push(cjk_punct);
                    continue;
                }
            }
        }
        result.push(c);
    }
    result
}

/// 检查是否在 Markdown 表格中（简化判断：所在行包含 |）
fn is_in_table(chars: &[char], pos: usize) -> bool {
    // 向前找到行首
    let mut start = pos;
    while start > 0 && chars[start - 1] != '\n' {
        start -= 1;
    }
    // 向后找到行尾
    let mut end = pos;
    while end < chars.len() && chars[end] != '\n' {
        end += 1;
    }
    let line: String = chars[start..end].iter().collect();
    line.contains('|')
}

/// 修复中英文括号混用
fn fix_mixed_brackets(text: &str, issues: &mut Vec<TypoIssue>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len());

    for (pos, &c) in chars.iter().enumerate() {
        let replacement = match c {
            '(' => {
                let has_cjk_after = pos + 1 < chars.len() && is_cjk(chars[pos + 1]);
                if has_cjk_after { Some('（') } else { None }
            }
            ')' => {
                let has_cjk_before = pos > 0 && is_cjk(chars[pos - 1]);
                if has_cjk_before { Some('）') } else { None }
            }
            _ => None,
        };

        if let Some(repl) = replacement
            && !is_in_url_or_code(&chars, pos)
            && !is_in_table(&chars, pos)
        {
            issues.push(TypoIssue::MixedBrackets {
                position: pos,
                found: c,
                expected: repl,
            });
            result.push(repl);
            continue;
        }
        result.push(c);
    }
    result
}

/// 修复中英文引号混用
fn fix_mixed_quotes(text: &str, issues: &mut Vec<TypoIssue>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len());

    for (pos, &c) in chars.iter().enumerate() {
        let replacement = match c {
            '"' => {
                // 判断是左引号还是右引号
                let has_cjk_after = pos + 1 < chars.len() && is_cjk(chars[pos + 1]);
                let has_cjk_before = pos > 0 && is_cjk(chars[pos - 1]);
                if has_cjk_before && !has_cjk_after {
                    Some('\u{201C}') // 左引号：前面是中文，后面不是
                } else if has_cjk_after && !has_cjk_before {
                    Some('\u{201D}') // 右引号：后面是中文，前面不是
                } else if has_cjk_before && has_cjk_after {
                    // 两边都是中文，简单处理为左引号
                    Some('\u{201C}')
                } else {
                    None
                }
            }
            '\'' => {
                let has_cjk_after = pos + 1 < chars.len() && is_cjk(chars[pos + 1]);
                let has_cjk_before = pos > 0 && is_cjk(chars[pos - 1]);
                if has_cjk_before && !has_cjk_after {
                    Some('\u{2018}') // 左单引号
                } else if has_cjk_after && !has_cjk_before {
                    Some('\u{2019}') // 右单引号
                } else if has_cjk_before && has_cjk_after {
                    Some('\u{2018}')
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(repl) = replacement
            && !is_in_url_or_code(&chars, pos)
            && !is_in_table(&chars, pos)
        {
            issues.push(TypoIssue::MixedQuotes {
                position: pos,
                found: c,
            });
            result.push(repl);
            continue;
        }
        result.push(c);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fix_cjk_latin_spacing() {
        let input = "字English字";
        let result = typo_lint(input);
        assert!(result.issue_count > 0);
        assert!(result.fixed_text.contains("字 English 字"));
    }

    #[test]
    fn test_fix_latin_punctuation() {
        let input = "字, 字.";
        let result = typo_lint(input);
        assert!(result.fixed_text.contains('，'));
        assert!(result.fixed_text.contains('。'));
    }

    #[test]
    fn test_fix_repeated_punctuation() {
        let input = "字。。。字。。";
        let result = typo_lint(input);
        // 连续重复标点被缩减
        assert!(!result.fixed_text.contains("。。"));
    }

    #[test]
    fn test_fix_mixed_brackets() {
        let input = "字(字)字";
        let result = typo_lint(input);
        assert!(result.fixed_text.contains('（'));
        assert!(result.fixed_text.contains('）'));
    }

    #[test]
    fn test_exempt_code_block() {
        let input = "```\nconsole.log(Hello World)\n```";
        let result = typo_lint(input);
        // 代码块内的内容不应被修改
        assert!(result.fixed_text.contains("console.log"));
    }

    #[test]
    fn test_exempt_url() {
        let input = "字 https://example.com 字";
        let result = typo_lint(input);
        // URL 不应被修改
        assert!(result.fixed_text.contains("https://example.com"));
    }

    #[test]
    fn test_exempt_table() {
        let input = "| 列A | 列B |\n|-----|-----|";
        let result = typo_lint(input);
        // 表格应保持原样
        assert!(result.fixed_text.contains("|"));
    }

    #[test]
    fn test_no_false_positive_on_punctuation_after_cjk() {
        // CJK 标点后紧跟 CJK，不应插入空格
        let input = "字。字。";
        let result = typo_lint(input);
        // 不应有 MissingSpaceBetweenCjkAndLatin 的问题
        assert!(
            !result
                .issues
                .iter()
                .any(|i| matches!(i, TypoIssue::MissingSpaceBetweenCjkAndLatin { .. }))
        );
    }

    #[test]
    fn test_fix_english_quotes() {
        let input = r#"字"Hello"字"#;
        let result = typo_lint(input);
        // 引号前后是 CJK，应替换为中文引号
        assert!(result.fixed_text.contains('\u{201C}') || result.fixed_text.contains('\u{201D}'));
    }

    #[test]
    fn test_leading_trailing_spaces() {
        let input = "  字\n字  \n字";
        let result = typo_lint(input);
        assert!(result.issues.iter().any(|i| matches!(
            i,
            TypoIssue::LeadingTrailingSpace {
                is_leading: true,
                ..
            }
        )));
        assert!(result.issues.iter().any(|i| matches!(
            i,
            TypoIssue::LeadingTrailingSpace {
                is_leading: false,
                ..
            }
        )));
    }

    #[test]
    fn test_empty_input() {
        let result = typo_lint("");
        assert_eq!(result.issue_count, 0);
        assert_eq!(result.fixed_text, "");
    }

    // ── 边界测试 ──

    #[test]
    fn test_pure_english() {
        let input = "This is pure English text without any CJK characters.";
        let result = typo_lint(input);
        // 纯英文不应触发CJK相关规则
        assert_eq!(result.issue_count, 0);
    }

    #[test]
    fn test_pure_chinese() {
        let input = "字字字字字。";
        let result = typo_lint(input);
        assert_eq!(result.issue_count, 0);
    }

    #[test]
    fn test_pure_numbers() {
        let input = "1234567890 3.14159 2024";
        let result = typo_lint(input);
        assert_eq!(result.issue_count, 0);
    }

    #[test]
    fn test_special_chars_only() {
        let input = "!@#$%^&*()_+-=[]{}|;':\",./<>?";
        let result = typo_lint(input);
        assert_eq!(result.issue_count, 0);
    }

    #[test]
    fn test_long_text() {
        let input = "文本A和Latin混合的文本。".repeat(1000);
        let result = typo_lint(&input);
        assert!(!result.fixed_text.is_empty());
    }

    #[test]
    fn test_only_exemptions() {
        // 整个文本都是豁免区域
        let input = "```code\nlet x = 1;\n```\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\nhttps://example.com/path";
        let result = typo_lint(input);
        assert_eq!(result.issue_count, 0);
    }

    #[test]
    fn test_mixed_quotes_boundary() {
        // 边界：只有单侧引号
        let input = "文本\"内容";
        let result = typo_lint(input);
        assert!(
            result
                .issues
                .iter()
                .any(|i| matches!(i, TypoIssue::MixedQuotes { .. }))
        );
    }

    #[test]
    fn test_repeated_punctuation_many() {
        let input = "文本。。。。。。";
        let result = typo_lint(input);
        // fix_repeated_punctuation 缩减重复标点但不记录 issue
        assert!(!result.fixed_text.contains("。。"));
    }
}
