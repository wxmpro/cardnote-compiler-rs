mod card_lint;
mod metrics;
pub mod pdf_postprocess;
mod preprocess;
mod report;
pub mod typo_lint;

pub use card_lint::*;
pub use metrics::*;
pub use pdf_postprocess::*;
pub use preprocess::clean_text;
pub use report::*;
pub use typo_lint::*;

/// 运行完整质量检测，返回报告
pub fn analyze(text: &str) -> QualityReport {
    QualityReport {
        character_health: metrics::character_health(text),
        structural_integrity: metrics::structural_integrity(text),
        noise_pollution: metrics::noise_pollution(text),
        semantic_coherence: metrics::semantic_coherence(text),
        content_completeness: metrics::content_completeness(text),
    }
}

/// 打印质量报告到 stdout
pub fn print_report(report: &QualityReport) {
    report.print();
}
