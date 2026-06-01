use colored::*;

use crate::error::Result;

/// 运行环境诊断
pub async fn cmd_doctor() -> Result<()> {
    println!("{}", "cardc doctor — 环境诊断".bold());
    println!();

    // Python 版本检查
    let python_ok = check_python().await;
    println!(
        "  Python    {}",
        if python_ok {
            "✅".green()
        } else {
            "❌".red()
        }
    );

    // Provider 和 API 凭据检查（替代单纯检查 LLM_API_KEY）
    let creds = crate::providers::scan_credentials();
    if !creds.is_empty() {
        println!("  Provider  {} (已配置 {} 个)", "✅".green(), creds.len());
        for cred in creds.values() {
            let registry = crate::providers::ProviderRegistry::new();
            if let Some(provider) = registry.get(&cred.provider_id) {
                let protocol_str = provider.protocol.as_str();
                let support = if provider.protocol.is_supported() {
                    "✅".green()
                } else {
                    "⚠️".yellow()
                };
                println!("    • {} {} [{}]", provider.name, support, protocol_str);
            }
        }
    } else {
        println!("  Provider  {}", "⚠️ 未配置".yellow());
    }

    // 依赖检查
    let deps = vec![("pymupdf", "PyMuPDF"), ("markitdown", "MarkItDown")];
    for (module, name) in deps {
        let ok = check_python_module(module).await;
        println!(
            "  {}   {}",
            name,
            if ok { "✅".green() } else { "❌".red() }
        );
    }

    // MinerU（可选）
    let mineru_ok = check_python_module("mineru").await;
    println!(
        "  MinerU    {}",
        if mineru_ok {
            "✅".green()
        } else {
            "⚪ (可选)".dimmed()
        }
    );

    let tesseract_ok = check_command("tesseract").await;
    println!(
        "  Tesseract {}",
        if tesseract_ok {
            "✅".green()
        } else {
            "⚪ (fallback 可选)".dimmed()
        }
    );

    let pdfinfo_ok = check_command("pdfinfo").await;
    let pdftoppm_ok = check_command("pdftoppm").await;
    println!(
        "  Poppler    {}",
        if pdfinfo_ok && pdftoppm_ok {
            "✅".green()
        } else {
            "⚪ (Tesseract fallback 需要)".dimmed()
        }
    );

    let tesseract_lang_ok = check_tesseract_lang("chi_sim").await;
    println!(
        "  chi_sim    {}",
        if tesseract_lang_ok {
            "✅".green()
        } else {
            "⚪ (中文 OCR 需要: brew install tesseract-lang)".dimmed()
        }
    );

    println!();
    let has_provider = !creds.is_empty();
    let all_supported = creds.values().all(|cred| {
        let registry = crate::providers::ProviderRegistry::new();
        registry
            .get(&cred.provider_id)
            .map(|p| p.protocol.is_supported())
            .unwrap_or(false)
    });

    if python_ok && has_provider && all_supported {
        println!("{}", "✅ 环境就绪，可以开始编译".green().bold());
    } else {
        println!("{}", "⚠ 发现环境问题需处理".yellow().bold());
        if !has_provider {
            println!("  → 运行 'cardc init' 配置 LLM 提供商");
        }
        if !all_supported {
            println!("  → 部分提供商协议暂不支持，请使用 OpenAI-compatible API");
        }
    }

    Ok(())
}

async fn check_python() -> bool {
    tokio::process::Command::new("python3")
        .arg("--version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

async fn check_python_module(module: &str) -> bool {
    tokio::process::Command::new("python3")
        .args(["-c", &format!("import {}", module)])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

async fn check_command(command: &str) -> bool {
    tokio::process::Command::new(command)
        .arg("--version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

async fn check_tesseract_lang(lang: &str) -> bool {
    let output = tokio::process::Command::new("tesseract")
        .arg("--list-langs")
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .any(|line| line.trim() == lang),
        _ => false,
    }
}
