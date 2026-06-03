use regex::Regex;
use std::sync::LazyLock;

use crate::config::{DOC_LIMITS, doc_limits_for};
use crate::error::Result;
use crate::models::{LlmMessage, Summary};
use crate::stages::common::ChatFn;

/// 生成文档摘要
pub async fn generate_summary(
    document: &str,
    call_llm: impl ChatFn,
    load_prompt: &(dyn Fn(&str) -> Result<String> + Send + Sync),
) -> Result<Summary> {
    let prompt_template = load_prompt("summary")?;
    let prompt = prompt_template.replace("{document}", document);

    let system = "你是一位资深的内容分析师和知识策展人。".to_string();
    let user = prompt;

    let response = match call_llm
        .call_chat(
            vec![
                LlmMessage {
                    role: "system".to_string(),
                    content: system,
                },
                LlmMessage {
                    role: "user".to_string(),
                    content: user,
                },
            ],
            Some(doc_limits_for(document.chars().count()).summary_output as u32),
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("    ⚠ 摘要 LLM 调用失败，返回默认摘要: {}", e);
            return Ok(Summary::default());
        }
    };

    parse_summary(&response)
}

/// 合并多个摘要
pub async fn merge_summaries(
    summaries: &[Summary],
    _document: &str,
    call_llm: impl ChatFn,
    load_prompt: &(dyn Fn(&str) -> Result<String> + Send + Sync),
) -> Result<Summary> {
    if summaries.is_empty() {
        return Ok(Summary::default());
    }
    if summaries.len() == 1 {
        return Ok(summaries[0].clone());
    }

    let prompt_template = load_prompt("summary")?;
    let summaries_text: Vec<String> = summaries
        .iter()
        .map(|s| {
            format!(
                "## {}\n{}\n要点: {}",
                s.title,
                s.overview,
                s.key_points.join("; ")
            )
        })
        .collect();

    let prompt = format!(
        "以下是一篇长文档各部分的摘要，请合并为一份全局摘要：\n\n{}\n\n{}",
        summaries_text.join("\n\n"),
        prompt_template.replace("{document}", "以上是需要合并的各部分摘要")
    );

    let system = "你是一位资深的内容分析师，擅长整合多部分摘要。".to_string();

    let response = call_llm
        .call_chat(
            vec![
                LlmMessage {
                    role: "system".to_string(),
                    content: system,
                },
                LlmMessage {
                    role: "user".to_string(),
                    content: prompt,
                },
            ],
            Some(DOC_LIMITS.summary_output as u32),
        )
        .await?;

    parse_summary(&response)
}

// 预编译正则：解析摘要响应
static TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"#\s+(.+?)\s*—\s*核心摘要").expect("硬编码正则"));
static TITLE_FALLBACK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"#\s+(.+)").expect("硬编码正则"));
static OVERVIEW_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"##\s+概述\s*\n+([\s\S]+?)(?:\n##\s|$)").expect("硬编码正则"));
static POINTS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"##\s+核心要点\s*\n+([\s\S]+?)(?:\n##\s|$)").expect("硬编码正则"));
static STRUCTURE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"##\s+结构\s*\n+([\s\S]+?)(?:\n##\s|$)").expect("硬编码正则"));
static POINT_ITEM_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:\d+\.|[\-*])\s*(.+)$").expect("硬编码正则"));

/// 解析摘要响应
fn parse_summary(response: &str) -> Result<Summary> {
    let title_re = &*TITLE_RE;
    let title_fallback_re = &*TITLE_FALLBACK_RE;
    let overview_re = &*OVERVIEW_RE;
    let points_re = &*POINTS_RE;
    let structure_re = &*STRUCTURE_RE;

    let title = title_re
        .captures(response)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .or_else(|| {
            title_fallback_re
                .captures(response)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().trim().to_string())
        })
        .unwrap_or_else(|| "未命名".to_string());

    let overview = overview_re
        .captures(response)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_default();

    let key_points = if let Some(caps) = points_re.captures(response) {
        let points_text = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let point_item_re = &*POINT_ITEM_RE;
        points_text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| {
                point_item_re
                    .captures(l)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().trim().to_string())
            })
            .collect()
    } else {
        Vec::new()
    };

    let structure = structure_re
        .captures(response)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_default();

    Ok(Summary {
        title,
        overview,
        key_points,
        structure,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_summary_full() {
        let response = r#"# 标题A — 核心摘要

## 概述

概述A。

## 核心要点

1. 要点A
2. 要点B
- 要点C

## 结构

章节A
章节B
"#;
        let summary = parse_summary(response).unwrap();
        assert_eq!(summary.title, "标题A");
        assert_eq!(summary.overview, "概述A。");
        assert_eq!(summary.key_points.len(), 3);
        assert_eq!(summary.key_points[0], "要点A");
        assert_eq!(summary.key_points[1], "要点B");
        assert_eq!(summary.key_points[2], "要点C");
        assert!(!summary.structure.is_empty());
    }

    #[test]
    fn test_parse_summary_minimal() {
        let response = "# 标题B\n\n内容A";
        let summary = parse_summary(response).unwrap();
        assert_eq!(summary.title, "标题B");
    }

    #[test]
    fn test_parse_summary_no_overview() {
        let response = r#"# 标题C — 核心摘要

## 核心要点

1. 要点D
"#;
        let summary = parse_summary(response).unwrap();
        assert_eq!(summary.title, "标题C");
        assert!(summary.overview.is_empty());
        assert_eq!(summary.key_points.len(), 1);
    }

    #[test]
    fn test_parse_summary_fallback_title() {
        let response = "文本A";
        let summary = parse_summary(response).unwrap();
        assert_eq!(summary.title, "未命名");
    }

    #[test]
    fn test_parse_summary_bullet_points() {
        let response = r#"# 标题D — 核心摘要

## 核心要点

- 第一项
* 第二项
3. 第三项
"#;
        let summary = parse_summary(response).unwrap();
        assert_eq!(summary.key_points.len(), 3);
        assert_eq!(summary.key_points[0], "第一项");
        assert_eq!(summary.key_points[1], "第二项");
        assert_eq!(summary.key_points[2], "第三项");
    }
}
