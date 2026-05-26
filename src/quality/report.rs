use colored::*;

use super::metrics::*;

#[derive(Debug, Clone)]
pub struct QualityReport {
    pub character_health: CharacterHealth,
    pub structural_integrity: StructuralIntegrity,
    pub noise_pollution: NoisePollution,
    pub semantic_coherence: SemanticCoherence,
    pub content_completeness: ContentCompleteness,
}

impl QualityReport {
    pub fn overall_score(&self) -> f32 {
        (self.character_health.score
            + self.structural_integrity.score
            + self.noise_pollution.score
            + self.semantic_coherence.score
            + self.content_completeness.score)
            / 5.0
    }

    pub fn grade(&self) -> &'static str {
        match self.overall_score() {
            s if s >= 90.0 => "A",
            s if s >= 75.0 => "B",
            s if s >= 60.0 => "C",
            s if s >= 40.0 => "D",
            _ => "F",
        }
    }

    pub fn is_acceptable(&self) -> bool {
        self.overall_score() >= 60.0
            && self.character_health.score >= 50.0
            && self.noise_pollution.score >= 40.0
            && self.semantic_coherence.score >= 40.0
    }

    pub fn critical_issues(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if self.character_health.garbled_ratio > 0.05 {
            issues.push(format!(
                "乱码比例过高: {:.1}%",
                self.character_health.garbled_ratio * 100.0
            ));
        }
        if self.noise_pollution.watermark_detected {
            issues.push(format!(
                "检测到水印污染: '{}' 重复出现",
                self.noise_pollution.watermark_text
            ));
        }
        if self.noise_pollution.repeated_lines > 50 {
            issues.push(format!(
                "大量重复内容: {} 行重复",
                self.noise_pollution.repeated_lines
            ));
        }
        if self.semantic_coherence.forced_break_count > 500 {
            issues.push(format!(
                "强制换行较多: {} 处",
                self.semantic_coherence.forced_break_count
            ));
        }
        if self.semantic_coherence.broken_urls > 10 {
            issues.push(format!(
                "URL 断裂: {} 处 URL 被换行切断",
                self.semantic_coherence.broken_urls
            ));
        }
        if self.structural_integrity.has_toc {
            issues.push(format!(
                "目录混入正文: 检测到 {} 行目录格式内容",
                self.structural_integrity.toc_line_count
            ));
        }
        if self.content_completeness.blank_ratio > 0.1 {
            issues.push(format!(
                "空白区域过多: {:.1}%",
                self.content_completeness.blank_ratio * 100.0
            ));
        }

        issues
    }

    pub fn print(&self) {
        let score = self.overall_score();
        let grade = self.grade();

        println!(
            "\n{}",
            "╔════════════════════════════════════════════════════════════╗".bright_cyan()
        );
        println!(
            "{}",
            "║              📊 PDF 解析质量检测报告                       ║".bright_cyan()
        );
        println!(
            "{}",
            "╚════════════════════════════════════════════════════════════╝".bright_cyan()
        );

        let grade_color = match grade {
            "A" => grade.bright_green(),
            "B" => grade.green(),
            "C" => grade.yellow(),
            "D" => grade.bright_yellow(),
            _ => grade.red(),
        };
        println!("\n  综合评分: {:.1}/100  (等级: {})", score, grade_color);

        if self.is_acceptable() {
            println!("  {}", "✅ 质量可接受".green());
        } else {
            println!("  {}", "❌ 质量不合格，建议修复后重新处理".red());
        }

        // 1. 字符健康度
        println!("\n{}", "  ① 字符健康度".bold());
        println!("     总字符数: {}", self.character_health.total_chars);
        println!(
            "     可打印字符: {:.1}%",
            self.character_health.printable_ratio * 100.0
        );
        println!(
            "     CJK 占比: {:.1}%",
            self.character_health.cjk_ratio * 100.0
        );
        if self.character_health.garbled_ratio > 0.01 {
            println!(
                "     {} 乱码比例: {:.2}%",
                "⚠".yellow(),
                self.character_health.garbled_ratio * 100.0
            );
        }
        if self.character_health.replacement_char_count > 0 {
            println!(
                "     {} 替换字符: {} 个",
                "⚠".yellow(),
                self.character_health.replacement_char_count
            );
        }
        println!(
            "     评分: {:.0}/100 {}",
            self.character_health.score,
            score_bar(self.character_health.score)
        );

        // 2. 结构完整性
        println!("\n{}", "  ② 结构完整性".bold());
        println!(
            "     标题分布: H1={} H2={} H3={} H4={} H5={} H6={}",
            self.structural_integrity.heading_counts[0],
            self.structural_integrity.heading_counts[1],
            self.structural_integrity.heading_counts[2],
            self.structural_integrity.heading_counts[3],
            self.structural_integrity.heading_counts[4],
            self.structural_integrity.heading_counts[5],
        );
        println!(
            "     段落数: {}  平均长度: {:.0} 字",
            self.structural_integrity.paragraph_count,
            self.structural_integrity.avg_paragraph_length
        );
        if self.structural_integrity.short_paragraphs > 50 {
            println!(
                "     {} 短段落过多: {} 个 (<20字)",
                "⚠".yellow(),
                self.structural_integrity.short_paragraphs
            );
        }
        if self.structural_integrity.has_toc {
            println!(
                "     {} 检测到目录混入正文: {} 行",
                "⚠".yellow(),
                self.structural_integrity.toc_line_count
            );
        }
        println!(
            "     评分: {:.0}/100 {}",
            self.structural_integrity.score,
            score_bar(self.structural_integrity.score)
        );

        // 3. 噪声污染度
        println!("\n{}", "  ③ 噪声污染度".bold());
        println!("     重复行数: {} 行", self.noise_pollution.repeated_lines);
        if !self.noise_pollution.top_repeated_phrases.is_empty() {
            println!("     高频重复短语:");
            for (phrase, count) in &self.noise_pollution.top_repeated_phrases {
                let preview = if phrase.chars().count() > 40 {
                    format!("{}...", phrase.chars().take(40).collect::<String>())
                } else {
                    phrase.clone()
                };
                println!("       ×{}  '{}'", count, preview.bright_black());
            }
        }
        if self.noise_pollution.watermark_detected {
            println!(
                "     {} 检测到水印: '{}'",
                "⚠".yellow(),
                self.noise_pollution.watermark_text.bright_yellow()
            );
        }
        if self.noise_pollution.page_break_count > 0 {
            println!("     分页符: {} 个", self.noise_pollution.page_break_count);
        }
        println!(
            "     评分: {:.0}/100 {}",
            self.noise_pollution.score,
            score_bar(self.noise_pollution.score)
        );

        // 4. 语义连贯性
        println!("\n{}", "  ④ 语义连贯性".bold());
        println!(
            "     强制换行: {} 处",
            self.semantic_coherence.forced_break_count
        );
        println!("     断句: {} 处", self.semantic_coherence.broken_sentences);
        println!("     URL 断裂: {} 处", self.semantic_coherence.broken_urls);
        println!(
            "     平均行长度: {:.0} 字  短行比例: {:.1}%",
            self.semantic_coherence.avg_line_length,
            self.semantic_coherence.short_line_ratio * 100.0
        );
        println!(
            "     评分: {:.0}/100 {}",
            self.semantic_coherence.score,
            score_bar(self.semantic_coherence.score)
        );

        // 5. 内容完整性
        println!("\n{}", "  ⑤ 内容完整性".bold());
        println!(
            "     总字符数: {}  估算页数: ~{} 页",
            self.content_completeness.total_chars,
            self.content_completeness.non_empty_pages_estimate
        );
        println!(
            "     内容密度: {:.0} 字/页",
            self.content_completeness.content_density
        );
        if self.content_completeness.blank_ratio > 0.05 {
            println!(
                "     {} 空白区域: {:.1}%",
                "⚠".yellow(),
                self.content_completeness.blank_ratio * 100.0
            );
        }
        if self.content_completeness.has_references {
            println!(
                "     参考文献: {} 处引用",
                self.content_completeness.reference_count
            );
        }
        println!(
            "     评分: {:.0}/100 {}",
            self.content_completeness.score,
            score_bar(self.content_completeness.score)
        );

        // 关键问题汇总
        let issues = self.critical_issues();
        if !issues.is_empty() {
            println!("\n  {}", "🔴 关键问题".red().bold());
            for issue in &issues {
                println!("     {} {}", "•".red(), issue);
            }
        }

        println!();
    }
}

fn score_bar(score: f32) -> String {
    let filled = (score / 10.0) as usize;
    let empty = 10 - filled.min(10);
    let bar = "█".repeat(filled) + &"░".repeat(empty);
    match score {
        s if s >= 80.0 => format!("[{}]", bar.bright_green()),
        s if s >= 60.0 => format!("[{}]", bar.green()),
        s if s >= 40.0 => format!("[{}]", bar.yellow()),
        _ => format!("[{}]", bar.red()),
    }
}
