use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;

use chrono::Local;
use tokio::fs;
use tokio::process::Command;

use crate::config::TIMESTAMP_FORMAT;
use crate::error::Result;
use crate::models::{
    Card, CardStatus, CompilationDiagnostics, CompilationResult, Entity, Relation, Summary,
};
use crate::quality::QualityReport;
use crate::scan::ScanResult;

/// 可选：使用 42md 的 lint 工具优化排版
async fn apply_42md_lint(md_path: &Path) -> Result<()> {
    // 检查 42md 是否可用
    let check = Command::new("42md")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    let is_available = match check {
        Ok(status) => status.success(),
        Err(_) => false,
    };
    if !is_available {
        return Ok(()); // 42md 未安装，静默跳过
    }

    let output = Command::new("42md")
        .args(["tools", "lint", "--fix"])
        .arg(md_path)
        .output()
        .await;

    if let Ok(out) = output
        && out.status.success()
    {
        eprintln!("  ✓ 已应用 42md lint 排版优化");
    }
    Ok(())
}

/// 可选：使用 42md 将 Markdown 转为 PDF
#[allow(dead_code)]
async fn apply_42md_md2pdf(md_path: &Path) -> Result<()> {
    let check = Command::new("42md")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    let is_available = match check {
        Ok(status) => status.success(),
        Err(_) => false,
    };
    if !is_available {
        return Ok(());
    }

    let output = Command::new("42md")
        .args(["tools", "md2pdf", "--no-remote-images"])
        .arg(md_path)
        .output()
        .await;

    if let Ok(out) = output
        && out.status.success()
    {
        eprintln!("  ✓ 已生成 PDF (42md md2pdf)");
    }
    Ok(())
}

/// 保存输入质量报告
pub async fn save_input_quality_report(
    output_dir: &str,
    source_file: &str,
    scan: Option<&ScanResult>,
    report: &QualityReport,
) -> Result<String> {
    let title = Path::new(source_file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("input_quality");
    let dir = create_output_dir(output_dir, Some(title)).await?;
    let report_path = dir.join("input_quality_report.md");

    let mut content = String::new();
    content.push_str("# 输入质量报告\n\n");
    content.push_str(&format!("- 来源文件：{}\n", source_file));
    content.push_str(&format!("- 综合等级：{}\n", report.grade()));
    content.push_str(&format!("- 综合评分：{:.1}/100\n", report.overall_score()));
    content.push_str(&format!(
        "- 是否允许进入编译：{}\n",
        if report.is_acceptable() { "是" } else { "否" }
    ));

    if let Some(scan) = scan {
        content.push_str("\n## PDF 探测\n\n");
        content.push_str(&format!("- 页数：{}\n", scan.pages));
        content.push_str(&format!("- 抽样页数：{}\n", scan.sampled_pages));
        content.push_str(&format!("- 文本密度：{:.1} 字/页\n", scan.text_density));
        content.push_str(&format!(
            "- 需要 OCR：{}\n",
            if scan.requires_ocr { "是" } else { "否" }
        ));
        content.push_str(&format!("- 置信度：{}%\n", scan.confidence));
        content.push_str(&format!("- 探测方法：{}\n", scan.detection_method));
        content.push_str(&format!("- 判断原因：{}\n", scan.reason));
    }

    let issues = report.critical_issues();
    if !issues.is_empty() {
        content.push_str("\n## 关键问题\n\n");
        for issue in issues {
            content.push_str(&format!("- {}\n", issue));
        }
    }

    fs::create_dir_all(&dir).await?;
    fs::write(&report_path, content).await?;
    Ok(dir.to_string_lossy().to_string())
}

/// 保存单篇编译结果
pub async fn save_single(result: &CompilationResult, output_dir: &str) -> Result<String> {
    // 优先使用源文件名（PDF 名称/书名）作为目录名
    let title = if !result.source_file.is_empty() {
        Path::new(&result.source_file)
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
    } else {
        None
    };
    let title = title.or_else(|| {
        if !result.summary.title.is_empty() && result.summary.title != "未命名" {
            Some(result.summary.title.as_str())
        } else {
            None
        }
    });
    let dir = create_output_dir(output_dir, title).await?;

    // 保存摘要
    let summary_path = dir.join("summary.md");
    fs::write(&summary_path, result.summary.to_markdown()).await?;
    apply_42md_lint(&summary_path).await.ok();

    // 保存卡片(按类型分组)
    let cards_dir = dir.join("cards");
    fs::create_dir_all(&cards_dir).await?;
    save_cards_by_type(&cards_dir, &result.cards).await?;

    // 保存所有卡片(单文件)
    let all_cards_path = dir.join("all_cards.md");
    let all_cards_content: Vec<String> = result
        .cards
        .iter()
        .map(|c| {
            let mut card = c.clone();
            card.typo_fix();
            card.to_markdown()
        })
        .collect();
    fs::write(&all_cards_path, all_cards_content.join("\n")).await?;

    let quality_path = dir.join("card_quality_report.md");
    fs::write(&quality_path, cards_quality_report(&result.cards)).await?;

    // 保存图谱
    let graph_path = dir.join("graph.mmd");
    fs::write(&graph_path, result.graph.to_mermaid()).await?;

    // 保存实体列表
    let entities_path = dir.join("entities.md");
    fs::write(&entities_path, entities_to_markdown(&result.graph.entities)).await?;

    // 保存分块信息(如果有)
    if !result.chunks.is_empty() {
        let chunks_path = dir.join("chunks.md");
        fs::write(&chunks_path, chunks_to_markdown(&result.chunks)).await?;
    }

    // 保存编译诊断报告
    if !result.diagnostics.failures.is_empty()
        || !result.diagnostics.degradations.is_empty()
        || !result.diagnostics.retries.is_empty()
    {
        let diag_path = dir.join("compile_diagnostics.md");
        fs::write(
            &diag_path,
            compilation_diagnostics_to_markdown(&result.diagnostics),
        )
        .await?;
    }

    Ok(dir.to_string_lossy().to_string())
}

/// 保存书籍级编译结果
pub async fn save_book(
    global_summary: &Summary,
    all_cards: &[Card],
    all_entities: &[Entity],
    all_relations: &[Relation],
    output_dir: &str,
    book_title: &str,
) -> Result<String> {
    let dir = create_output_dir(output_dir, Some(book_title)).await?;

    // 保存全局摘要
    let summary_path = dir.join("summary.md");
    fs::write(&summary_path, global_summary.to_markdown()).await?;
    apply_42md_lint(&summary_path).await.ok();

    // 保存卡片
    let cards_dir = dir.join("cards");
    fs::create_dir_all(&cards_dir).await?;
    save_cards_by_type(&cards_dir, all_cards).await?;

    // 保存所有卡片
    let all_cards_path = dir.join("all_cards.md");
    let all_cards_content: Vec<String> = all_cards.iter().map(|c| c.to_markdown()).collect();
    fs::write(&all_cards_path, all_cards_content.join("\n")).await?;

    let quality_path = dir.join("card_quality_report.md");
    fs::write(&quality_path, cards_quality_report(all_cards)).await?;

    // 保存实体
    let entities_path = dir.join("entities.md");
    fs::write(&entities_path, entities_to_markdown(all_entities)).await?;

    // 保存关系图谱
    let graph_path = dir.join("graph.mmd");
    let graph = crate::models::KnowledgeGraph {
        entities: all_entities.to_vec(),
        relations: all_relations.to_vec(),
    };
    fs::write(&graph_path, graph.to_mermaid()).await?;

    Ok(dir.to_string_lossy().to_string())
}


/// 保存单篇编译结果到指定目录（不创建新的带时间戳的目录）
/// 保存单篇编译结果到指定目录
pub async fn save_single_to_dir(result: &CompilationResult, output_dir: &Path) -> Result<()> {
    fs::create_dir_all(output_dir).await?;

    // 保存摘要
    let summary_path = output_dir.join("summary.md");
    fs::write(&summary_path, result.summary.to_markdown()).await?;
    apply_42md_lint(&summary_path).await.ok();

    // 保存卡片(按类型分组)
    let cards_dir = output_dir.join("cards");
    fs::create_dir_all(&cards_dir).await?;
    save_cards_by_type(&cards_dir, &result.cards).await?;

    // 保存所有卡片(单文件)
    let all_cards_path = output_dir.join("all_cards.md");
    let all_cards_content: Vec<String> = result
        .cards
        .iter()
        .map(|c| {
            let mut card = c.clone();
            card.typo_fix();
            card.to_markdown()
        })
        .collect();
    fs::write(&all_cards_path, all_cards_content.join("\n")).await?;

    let quality_path = output_dir.join("card_quality_report.md");
    fs::write(&quality_path, cards_quality_report(&result.cards)).await?;

    // 保存图谱
    let graph_path = output_dir.join("graph.mmd");
    fs::write(&graph_path, result.graph.to_mermaid()).await?;

    // 保存实体列表
    let entities_path = output_dir.join("entities.md");
    fs::write(&entities_path, entities_to_markdown(&result.graph.entities)).await?;

    // 保存分块信息(如果有)
    if !result.chunks.is_empty() {
        let chunks_path = output_dir.join("chunks.md");
        fs::write(&chunks_path, chunks_to_markdown(&result.chunks)).await?;
    }

    // 保存编译诊断报告
    if !result.diagnostics.failures.is_empty()
        || !result.diagnostics.degradations.is_empty()
        || !result.diagnostics.retries.is_empty()
    {
        let diag_path = output_dir.join("compile_diagnostics.md");
        fs::write(
            &diag_path,
            compilation_diagnostics_to_markdown(&result.diagnostics),
        )
        .await?;
    }

    Ok(())
}

/// 按类型保存卡片
pub async fn save_cards_by_type(dir: &Path, cards: &[Card]) -> Result<()> {
    let mut by_type: HashMap<String, Vec<Card>> = HashMap::new();
    let mut total_fixes = 0;
    for card in cards {
        let mut cloned = card.clone();
        total_fixes += cloned.typo_fix();
        by_type
            .entry(card.card_type.to_string())
            .or_default()
            .push(cloned);
    }
    for (card_type, cards_of_type) in by_type {
        let filename = format!("{}.md", card_type);
        let path = dir.join(&filename);
        let content: Vec<String> = cards_of_type.iter().map(|c| c.to_markdown()).collect();
        fs::write(&path, content.join("\n")).await?;
    }

    if total_fixes > 0 {
        eprintln!("  ✓ 中文排版修复: {} 处问题已自动修正", total_fixes);
    }

    Ok(())
}

/// 创建输出目录，目录名冲突时自动追加递增序号
/// 创建输出目录
pub async fn create_output_dir(base: &str, title: Option<&str>) -> Result<std::path::PathBuf> {
    let dir = resolve_unique_output_dir(base, title);
    fs::create_dir_all(&dir).await?;
    Ok(dir)
}

/// 生成唯一的输出目录路径（处理时间戳冲突）
fn resolve_unique_output_dir(base: &str, title: Option<&str>) -> std::path::PathBuf {
    let timestamp = Local::now().format(TIMESTAMP_FORMAT).to_string();
    let base_name = if let Some(t) = title {
        let safe = sanitize_filename(t);
        format!("{}_{}", timestamp, safe)
    } else {
        timestamp
    };
    resolve_unique_output_dir_raw(base, &base_name)
}

/// 内部实现：基于固定 base_name 解决目录冲突
fn resolve_unique_output_dir_raw(base: &str, base_name: &str) -> std::path::PathBuf {
    let base_path = Path::new(base);
    let dir = base_path.join(base_name);
    if !dir.exists() {
        return dir;
    }

    let mut counter = 1;
    loop {
        let candidate = base_path.join(format!("{}_{:03}", base_name, counter));
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

/// 卡片质量报告转 Markdown
fn cards_quality_report(cards: &[Card]) -> String {
    let mut lines = vec!["# 卡片质量报告\n".to_string()];
    lines.push(format!("- 卡片总数：{}", cards.len()));

    let avg_score = if cards.is_empty() {
        0.0
    } else {
        cards.iter().map(|c| c.quality_score).sum::<f64>() / cards.len() as f64
    };
    lines.push(format!("- 平均质量分：{:.0}%", avg_score * 100.0));

    let mut by_status: HashMap<&'static str, usize> = HashMap::new();
    let mut by_type: HashMap<String, usize> = HashMap::new();
    for card in cards {
        let status = match card.status {
            CardStatus::Accepted => "通过",
            CardStatus::NeedsRetry => "需重试",
            CardStatus::Degraded => "已降级",
            CardStatus::Rejected => "已拒绝",
        };
        *by_status.entry(status).or_insert(0) += 1;
        *by_type
            .entry(card.card_type.as_str().to_string())
            .or_insert(0) += 1;
    }

    lines.push("\n## 状态分布\n".to_string());
    for (status, count) in by_status {
        lines.push(format!("- {}：{} 张", status, count));
    }

    lines.push("\n## 类型分布\n".to_string());
    for (card_type, count) in by_type {
        lines.push(format!("- {}：{} 张", card_type, count));
    }

    let flagged: Vec<_> = cards
        .iter()
        .filter(|c| c.status != CardStatus::Accepted || !c.reject_reason.is_empty())
        .collect();
    if !flagged.is_empty() {
        lines.push("\n## 需关注卡片\n".to_string());
        for card in flagged {
            lines.push(format!(
                "- **{}** / {} / {:.0}%：{}",
                card.title,
                card.card_type,
                card.quality_score * 100.0,
                card.reject_reason
            ));
        }
    }

    lines.join("\n")
}

/// 编译诊断转 Markdown
fn compilation_diagnostics_to_markdown(diag: &CompilationDiagnostics) -> String {
    let mut lines = vec!["# 编译诊断报告\n".to_string()];

    if !diag.failures.is_empty() {
        lines.push("\n## ❌ 失败阶段\n".to_string());
        for fail in &diag.failures {
            lines.push(format!("- **阶段**: {}", fail.stage));
            lines.push(format!("  - **错误**: {}", fail.error));
            lines.push(format!("  - **重试次数**: {}", fail.retry_count));
            lines.push(format!("  - **最终状态**: {}\n", fail.final_status));
        }
    }

    if !diag.degradations.is_empty() {
        lines.push("\n## ⚠️ 降级阶段\n".to_string());
        for deg in &diag.degradations {
            lines.push(format!("- **阶段**: {}", deg.stage));
            lines.push(format!("  - **原因**: {}", deg.reason));
            lines.push(format!(
                "  - **预期产出**: {} 个 / **实际产出**: {} 个\n",
                deg.expected_count, deg.actual_count
            ));
        }
    }

    if !diag.retries.is_empty() {
        lines.push("\n## 🔄 重试结果\n".to_string());
        for retry in &diag.retries {
            let status = if retry.success {
                "✓ 成功"
            } else {
                "✗ 失败"
            };
            lines.push(format!(
                "- **阶段**: {} | **重试次数**: {} | **结果**: {}\n",
                retry.stage, retry.retry_count, status
            ));
        }
    }

    if diag.failures.is_empty() && diag.degradations.is_empty() && diag.retries.is_empty() {
        lines.push("\n✓ 编译过程无异常，所有阶段正常完成。\n".to_string());
    }

    lines.join("")
}

/// 实体列表转 Markdown
fn entities_to_markdown(entities: &[Entity]) -> String {
    let mut lines = vec!["# 实体列表\n".to_string()];
    for entity in entities {
        lines.push(format!(
            "- **{}** ({}){}",
            entity.name,
            entity.entity_type,
            if entity.context.is_empty() {
                "".to_string()
            } else {
                format!(" — {}", entity.context)
            }
        ));
    }
    lines.join("\n")
}

/// 分块信息转 Markdown
fn chunks_to_markdown(chunks: &[crate::models::ChunkInfo]) -> String {
    let mut lines = vec!["# 分块信息\n".to_string()];
    for (i, chunk) in chunks.iter().enumerate() {
        lines.push(format!(
            "## 块 {}: {}\n- 大小: {} 字符\n- 实体: {}\n- 卡片: {}\n- 关系: {}\n",
            i + 1,
            chunk.title_path,
            chunk.size,
            chunk.entities,
            chunk.cards,
            chunk.relations
        ));
    }
    lines.join("\n")
}

/// 清理文件名
/// 清理文件名中的非法字符
pub fn sanitize_filename(name: &str) -> String {
    name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
        .replace('：', "_")
        .replace('？', "_")
        .replace('"', "_")
        .replace('＜', "_")
        .replace('＞', "_")
        .replace('｜', "_")
        .replace('/', "_")
        .replace('\\', "_")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_resolve_unique_no_conflict() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().to_str().unwrap();
        let dir = resolve_unique_output_dir_raw(base, "20260528_120000_测试");
        assert!(dir.to_string_lossy().ends_with("20260528_120000_测试"));
    }

    #[test]
    fn test_resolve_unique_conflict_once() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().to_str().unwrap();

        let dir1 = resolve_unique_output_dir_raw(base, "conflict");
        fs::create_dir_all(&dir1).unwrap();

        let dir2 = resolve_unique_output_dir_raw(base, "conflict");
        assert_ne!(dir1, dir2);
        assert!(dir2.to_string_lossy().ends_with("conflict_001"));
    }

    #[test]
    fn test_resolve_unique_conflict_multiple() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().to_str().unwrap();

        for i in 0..3 {
            let dir = resolve_unique_output_dir_raw(base, "multi");
            fs::create_dir_all(&dir).unwrap();
            if i == 0 {
                assert!(dir.to_string_lossy().ends_with("multi"));
            } else {
                assert!(dir.to_string_lossy().ends_with(&format!("multi_{:03}", i)));
            }
        }

        let dir4 = resolve_unique_output_dir_raw(base, "multi");
        assert!(dir4.to_string_lossy().ends_with("multi_003"));
    }

    #[test]
    fn test_resolve_unique_gaps_reused() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().to_str().unwrap();

        // 创建 base, _001, _003（跳过 _002）
        fs::create_dir_all(resolve_unique_output_dir_raw(base, "gap")).unwrap();
        fs::create_dir_all(
            resolve_unique_output_dir_raw(base, "gap")
                .parent()
                .unwrap()
                .join("gap_001"),
        )
        .unwrap();
        fs::create_dir_all(
            resolve_unique_output_dir_raw(base, "gap")
                .parent()
                .unwrap()
                .join("gap_003"),
        )
        .unwrap();

        let next = resolve_unique_output_dir_raw(base, "gap");
        // 应取 _002（第一个不存在的序号）
        assert!(next.to_string_lossy().ends_with("gap_002"));
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("a/b:c"), "a_b_c");
        assert_eq!(sanitize_filename("  标题  "), "标题");
        assert_eq!(sanitize_filename("test|file?"), "test_file_");
    }
}
