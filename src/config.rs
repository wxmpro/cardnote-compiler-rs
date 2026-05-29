use std::collections::HashSet;
use std::io::{self, Write};
use std::path::PathBuf;

use colored::*;

use crate::error::{AppError, Result};
use crate::providers::{
    CredentialSource, ProviderCredential, ProviderRegistry, generate_status_report,
    scan_credentials,
};

// ═══════════════════════════════════════════════════════
//  分块配置
// ═══════════════════════════════════════════════════════

/// 单块最大字符数，超过则启用 Map-Reduce
pub const CHUNK_SIZE: usize = 50000;
/// 并行编译的最大线程数（避免触发 API 限流）
pub const MAX_WORKERS: usize = 1;

// ═══════════════════════════════════════════════════════
//  LLM 调用配置
// ═══════════════════════════════════════════════════════

/// 各阶段输入输出限制
pub struct DocLimits {
    pub summary_input: usize,
    pub summary_output: usize,
    pub entity_output: usize,
    pub graph_output: usize,
    pub card_output: usize,
    pub index_output: usize,
    pub compile_output: usize,
}

/// 默认文档限制（中文文本）
pub const DOC_LIMITS: DocLimits = DocLimits {
    summary_input: 12000,
    summary_output: 2000,
    entity_output: 8000,
    graph_output: 8000,
    card_output: 8000,
    index_output: 4000,
    compile_output: 8000,
};

/// 输出文件时间戳格式
pub const TIMESTAMP_FORMAT: &str = "%Y%m%d%H%M%S";

// ═══════════════════════════════════════════════════════
//  PDF 配置
// ═══════════════════════════════════════════════════════

pub const PDF_SAMPLE_PAGES: usize = 3;
pub const PDF_LARGE_SAMPLE_PAGES: usize = 5;
pub const PDF_LARGE_FILE_MB: u64 = 50;
pub const PDF_FALLBACK_MIN_TEXT: usize = 200;
pub const PDF_CONVERT_TIMEOUT: u64 = 600;
pub const PDF_SPLIT_PAGES_DEFAULT: usize = 100;
pub const PDF_TOC_MIN_ENTRIES: usize = 5;

// ═══════════════════════════════════════════════════════
//  OOM 防护配置
// ═══════════════════════════════════════════════════════

/// 全局文件大小上限（MB），超过则拒绝处理
pub const MAX_FILE_SIZE_MB: u64 = 500;
/// 文本文件流式读取阈值（MB），超过则逐行读取避免一次性加载
pub const MAX_TEXT_FILE_STREAM_MB: u64 = 50;
/// 外部命令输出大小上限（MB），超过则截断或报错
pub const MAX_COMMAND_OUTPUT_MB: u64 = 100;

// ═══════════════════════════════════════════════════════
//  文件格式判断
// ═══════════════════════════════════════════════════════

pub fn is_text_format(suffix: &str) -> bool {
    matches!(suffix, ".md" | ".txt" | ".markdown" | ".rst" | ".org")
}

pub fn is_pdf_format(suffix: &str) -> bool {
    suffix == ".pdf"
}

pub fn is_other_format(suffix: &str) -> bool {
    matches!(
        suffix,
        ".docx"
            | ".doc"
            | ".html"
            | ".htm"
            | ".epub"
            | ".pptx"
            | ".xlsx"
            | ".csv"
            | ".json"
            | ".xml"
            | ".rtf"
    )
}

// ═══════════════════════════════════════════════════════
//  多提供商配置管理
// ═══════════════════════════════════════════════════════

/// 应用配置（支持多提供商）
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub api_key: String,
    pub provider: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
}

impl AppConfig {
    /// 从环境变量加载配置
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("LLM_API_KEY").ok()?;
        let provider = std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "deepseek".to_string());
        let model = std::env::var("LLM_MODEL").ok();
        let base_url = std::env::var("LLM_BASE_URL").ok();

        Some(Self {
            api_key,
            provider,
            model,
            base_url,
        })
    }

    /// 从扫描到的凭据创建配置
    pub fn from_credential(cred: &ProviderCredential) -> Self {
        Self {
            api_key: cred.api_key.clone(),
            provider: cred.provider_id.clone(),
            model: cred.default_model.clone(),
            base_url: cred.base_url.clone(),
        }
    }
}

/// 提供商配置信息（用于显示）
pub struct ProviderConfig {
    pub provider: &'static str,
    pub base_url: &'static str,
    pub default_model: &'static str,
}

/// 获取提供商默认配置
pub fn get_provider_config(provider: &str) -> Option<ProviderConfig> {
    let registry = ProviderRegistry::new();
    let p = registry.find_by_alias(provider)?;
    let default_model = p.default_model()?;
    Some(ProviderConfig {
        provider: Box::leak(p.id.clone().into_boxed_str()),
        base_url: Box::leak(p.default_base_url.clone().into_boxed_str()),
        default_model: Box::leak(default_model.id.clone().into_boxed_str()),
    })
}

// ═══════════════════════════════════════════════════════
//  引导式配置向导
// ═══════════════════════════════════════════════════════

/// 运行引导式配置
///
/// 1. 自动扫描环境中已有的配置
/// 2. 显示配置状态
/// 3. 对未配置的提供商，引导用户添加
/// 4. 让用户选择默认使用的提供商
pub async fn interactive_setup() -> Result<(String, String, Option<String>, Option<String>)> {
    println!(
        "\n{}",
        "╔════════════════════════════════════════════════════════════╗".bright_cyan()
    );
    println!(
        "{}",
        "║        🤖 CardNote Compiler — AI 提供商配置向导          ║".bright_cyan()
    );
    println!(
        "{}",
        "╚════════════════════════════════════════════════════════════╝".bright_cyan()
    );

    // 第一步：自动扫描
    println!(
        "\n{}",
        "🔍 步骤 1/3: 正在扫描环境中的 AI 配置...".bright_yellow()
    );
    let mut credentials = scan_credentials();

    // 显示扫描结果
    println!("\n{}", generate_status_report(&credentials));

    // 第二步：引导添加未配置的提供商
    let registry = ProviderRegistry::new();
    let all_providers = registry.list_all();
    let configured_ids: HashSet<String> = credentials.keys().cloned().collect();

    let missing_providers: Vec<_> = all_providers
        .iter()
        .filter(|p| !configured_ids.contains(&p.id))
        .collect();

    if !missing_providers.is_empty() {
        println!("\n{}", "📋 步骤 2/3: 添加其他 AI 提供商".bright_yellow());
        println!(
            "{}\n",
            "发现以下提供商尚未配置，可按提示添加:".bright_black()
        );

        for provider in missing_providers {
            if let Some(cred) = prompt_add_provider(provider).await? {
                credentials.insert(provider.id.clone(), cred);
            }
        }
    } else {
        println!("\n✅ 所有支持的提供商均已配置！\n");
    }

    // 第三步：选择默认提供商
    println!(
        "{}",
        "📋 步骤 3/3: 选择默认使用的 AI 提供商".bright_yellow()
    );

    if credentials.is_empty() {
        println!("\n{}", "❌ 错误: 没有配置任何 AI 提供商".red());
        println!("{}", "请至少配置一个提供商后再运行。".bright_black());
        return Err(AppError::Config("没有可用的 AI 提供商".to_string()));
    }

    let selected = select_default_provider(&credentials).await?;
    let cred = credentials.get(&selected).unwrap();

    // 保存到 .env 文件
    save_to_env(cred)?;

    println!("\n{}", "✅ 配置完成！".green().bold());
    println!("   默认提供商: {}", selected.bright_cyan());
    println!(
        "   模型: {}\n",
        cred.default_model
            .as_deref()
            .unwrap_or("默认")
            .bright_cyan()
    );

    Ok((
        cred.api_key.clone(),
        selected,
        cred.default_model.clone(),
        cred.base_url.clone(),
    ))
}

/// 提示用户添加提供商
async fn prompt_add_provider(
    provider: &crate::providers::Provider,
) -> Result<Option<ProviderCredential>> {
    println!(
        "  {} {} — {}",
        "▶".bright_cyan(),
        provider.name.bold(),
        provider.description.bright_black()
    );
    println!(
        "    支持模型: {}",
        provider
            .models
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
            .bright_black()
    );
    println!(
        "    环境变量: {} 或配置文件中设置 api_key",
        provider.api_key_env_var.yellow()
    );

    print!("\n  是否添加 {}? [y/N/q(跳过全部)]: ", provider.name);
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let input = input.trim().to_lowercase();

    if input == "q" || input == "quit" {
        return Ok(None);
    }

    if input != "y" && input != "yes" {
        println!("  ⏭️  跳过 {}\n", provider.name);
        return Ok(None);
    }

    // 提示输入 API key
    print!("  请输入 {} API Key: ", provider.name);
    io::stdout().flush().unwrap();

    let mut api_key = String::new();
    io::stdin().read_line(&mut api_key).unwrap();
    let api_key = api_key.trim().to_string();

    if api_key.is_empty() {
        println!("  ⚠️  API Key 不能为空，跳过\n");
        return Ok(None);
    }

    if !provider.validate_key_format(&api_key) {
        println!(
            "  ⚠️  API Key 格式不正确，应以 '{}' 开头\n",
            provider.api_key_pattern
        );
        return Ok(None);
    }

    // 提示输入自定义 base_url（可选）
    print!(
        "  自定义 API 地址? [回车使用默认: {}]: ",
        provider.default_base_url
    );
    io::stdout().flush().unwrap();

    let mut base_url_input = String::new();
    io::stdin().read_line(&mut base_url_input).unwrap();
    let base_url = if base_url_input.trim().is_empty() {
        None
    } else {
        Some(base_url_input.trim().to_string())
    };

    // 选择模型
    println!("  可用模型:");
    for (i, model) in provider.models.iter().enumerate() {
        println!(
            "    {}. {} — {} (上下文: {}K)",
            i + 1,
            model.name,
            model.description,
            model.context_length / 1000
        );
    }
    print!("  选择模型 [1-{}，回车使用默认]: ", provider.models.len());
    io::stdout().flush().unwrap();

    let mut model_input = String::new();
    io::stdin().read_line(&mut model_input).unwrap();
    let default_model = if model_input.trim().is_empty() {
        provider.default_model().map(|m| m.id.clone())
    } else {
        match model_input.trim().parse::<usize>() {
            Ok(n) if n > 0 && n <= provider.models.len() => Some(provider.models[n - 1].id.clone()),
            _ => {
                println!("  ⚠️  无效选择，使用默认模型");
                provider.default_model().map(|m| m.id.clone())
            }
        }
    };

    println!("  ✅ {} 配置成功！\n", provider.name.green());

    Ok(Some(ProviderCredential {
        provider_id: provider.id.clone(),
        api_key,
        base_url,
        default_model,
        source: CredentialSource::UserInput,
    }))
}

/// 选择默认提供商
async fn select_default_provider(
    credentials: &std::collections::HashMap<String, ProviderCredential>,
) -> Result<String> {
    let registry = ProviderRegistry::new();

    println!("  已配置的提供商:");
    let providers: Vec<_> = credentials.keys().cloned().collect();

    for (i, id) in providers.iter().enumerate() {
        let provider = registry.get(id).unwrap();
        let cred = credentials.get(id).unwrap();
        let model_name = cred
            .default_model
            .as_ref()
            .and_then(|m| provider.find_model(m))
            .map(|m| m.name.as_str())
            .unwrap_or("默认");
        println!(
            "    {}. {} — {} ({}",
            i + 1,
            provider.name.bold(),
            provider.description,
            model_name
        );
    }

    print!("\n  选择默认提供商 [1-{}]: ", providers.len());
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    match input.trim().parse::<usize>() {
        Ok(n) if n > 0 && n <= providers.len() => Ok(providers[n - 1].clone()),
        _ => {
            println!("  无效选择，使用第一个已配置提供商");
            Ok(providers.first().unwrap().clone())
        }
    }
}

/// 保存配置到用户配置目录（~/.config/cardnote/.env）
/// 这样可以避免把 API Key 提交进 git 仓库
fn save_to_env(cred: &ProviderCredential) -> Result<()> {
    // 使用用户级配置目录而非项目根目录
    let config_dir = shellexpand::tilde("~/.config/cardnote");
    let config_path = std::path::PathBuf::from(config_dir.as_ref());
    std::fs::create_dir_all(&config_path).map_err(AppError::Io)?;

    let env_path = config_path.join(".env");
    let env_content = format!(
        "# CardNote Compiler 配置\n# 修改此文件来更改默认 LLM 提供商\n\nLLM_API_KEY={}\n\
         LLM_PROVIDER={}\n\
         LLM_MODEL={}\n",
        cred.api_key,
        cred.provider_id,
        cred.default_model.as_deref().unwrap_or("")
    );

    std::fs::write(&env_path, env_content).map_err(AppError::Io)?;

    println!(
        "  💾 配置已保存到: {}\n",
        env_path.display().to_string().bright_black()
    );
    Ok(())
}

/// 快速配置检查 — 用于 doctor 命令
pub async fn quick_config_check() -> Result<(String, String)> {
    // 1. 检查 .env 或环境变量
    if let Some(config) = AppConfig::from_env()
        && !config.api_key.is_empty()
        && config.api_key.len() > 10
    {
        return Ok((config.api_key, config.provider));
    }

    // 2. 扫描环境中的凭据
    let credentials = scan_credentials();
    if !credentials.is_empty() {
        // 自动选择第一个可用的
        let first_id = credentials.keys().next().unwrap().clone();
        let cred = credentials.get(&first_id).unwrap();
        return Ok((cred.api_key.clone(), first_id));
    }

    // 3. 引导式配置
    let (api_key, provider, _model, _base_url) = interactive_setup().await?;
    Ok((api_key, provider))
}

/// 获取可用的 API key 和提供商（优先使用 .env，否则扫描环境）
///
/// 如果提供了 api_key_arg 或 provider_arg，优先使用命令行参数。
/// 否则从 .env 文件或环境变量扫描。
/// 如果都找不到，启动交互式配置向导。
pub async fn get_api_config(
    api_key_arg: Option<String>,
    provider_arg: Option<String>,
) -> Result<(String, String)> {
    // 1. 同时提供了 api_key 和 provider — 直接使用
    if let Some(api_key) = api_key_arg {
        let provider = provider_arg.unwrap_or_else(|| "deepseek".to_string());
        return Ok((api_key, provider));
    }

    // 2. 优先扫描配置文件（统一配置优先于 .env）
    let credentials = scan_credentials();

    // 2a. 指定了 provider — 在配置文件中查找对应提供商
    if let Some(ref requested_provider) = provider_arg {
        if let Some(cred) = credentials.get(requested_provider) {
            println!(
                "{} 使用指定提供商 {} 配置 ({})",
                "✓".green(),
                requested_provider.bright_cyan(),
                match &cred.source {
                    CredentialSource::EnvVar => "环境变量",
                    CredentialSource::ConfigFile(_) => "配置文件",
                    CredentialSource::AutoDetected => "自动检测",
                    CredentialSource::UserInput => "手动输入",
                }
            );
            return Ok((cred.api_key.clone(), requested_provider.clone()));
        }

        // 配置文件中没有 — 回退到 .env 匹配
        if let Some(config) = AppConfig::from_env()
            && !config.api_key.is_empty()
            && config.api_key.len() > 10
            && config.provider == *requested_provider
        {
            return Ok((config.api_key, config.provider));
        }
    }

    // 2b. 未指定 provider — 配置文件中的第一个
    if !credentials.is_empty() {
        let first_id = credentials.keys().next().unwrap().clone();
        let cred = credentials.get(&first_id).unwrap();
        println!(
            "{} 自动检测到 {} 配置 ({})",
            "✓".green(),
            first_id.bright_cyan(),
            match &cred.source {
                CredentialSource::EnvVar => "环境变量",
                CredentialSource::ConfigFile(_) => "配置文件",
                CredentialSource::AutoDetected => "自动检测",
                CredentialSource::UserInput => "手动输入",
            }
        );
        return Ok((cred.api_key.clone(), first_id));
    }

    // 3. 配置文件为空 — 回退到 .env
    if let Some(config) = AppConfig::from_env()
        && !config.api_key.is_empty()
        && config.api_key.len() > 10
    {
        return Ok((config.api_key, config.provider));
    }

    // 5. 引导式配置
    println!("\n{} 未检测到任何 AI 提供商配置", "⚠".yellow());
    println!("{} 启动引导式配置向导...\n", "→".cyan());

    let (api_key, provider, _model, _base_url) = interactive_setup().await?;
    Ok((api_key, provider))
}

/// 查找 .env 文件（优先用户级配置，其次项目根目录及父目录）
pub fn find_env_file() -> Option<PathBuf> {
    // 优先查找用户级配置文件
    let user_env = PathBuf::from(shellexpand::tilde("~/.config/cardnote/.env").as_ref());
    if user_env.exists() {
        return Some(user_env);
    }

    // 回退到项目根目录及父目录
    let candidates = [".env", "../.env", "../../.env"];
    for name in &candidates {
        let path = PathBuf::from(name);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// 获取提供商显示名称
pub fn get_provider_label(provider: &str) -> String {
    let registry = ProviderRegistry::new();
    registry
        .find_by_alias(provider)
        .map(|p| p.name.to_string())
        .unwrap_or_else(|| provider.to_string())
}
