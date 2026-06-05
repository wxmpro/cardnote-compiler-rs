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
/// Deepseek V4 上下文 1M tokens，约可承载 50-60 万中文字符（含 prompt 开销）
pub const CHUNK_SIZE: usize = 500000;

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

/// 根据文档字符数动态计算各阶段 max_tokens
///
/// 逻辑：
/// - summary: 固定 2000（摘要始终是压缩任务）
/// - entities: min(max(doc_chars/20, 4000), 8000)  — JSON 输出需要较大空间
/// - graph:    固定 10000 — JSON 输出需要充足空间，避免截断
/// - cards:    min(max(doc_chars/8,  3000), 8000)
/// - index:    固定 4000
pub fn doc_limits_for(doc_chars: usize) -> DocLimits {
    let scale =
        |divisor: usize, min: usize, max: usize| -> usize { (doc_chars / divisor).clamp(min, max) };
    DocLimits {
        summary_input: 12000,
        summary_output: 2000,
        entity_output: scale(20, 4000, 8000),
        graph_output: 10000,
        card_output: scale(8, 3000, 8000),
        index_output: 4000,
        compile_output: scale(8, 3000, 8000),
    }
}

/// 向后兼容：静态默认值（文档 ≤ 50K 字符时与旧行为一致）
pub const DOC_LIMITS: DocLimits = DocLimits {
    summary_input: 12000,
    summary_output: 2000,
    entity_output: 8000,
    graph_output: 8000,
    card_output: 8000,
    index_output: 4000,
    compile_output: 8000,
};

// ═══════════════════════════════════════════════════════
//  Stage-level 模型配置（Tiered Strategy）
// ═══════════════════════════════════════════════════════

/// 各阶段可独立配置模型，空字符串表示使用默认模型
pub struct StageModelConfig {
    pub summary: Option<String>,
    pub entities: Option<String>,
    pub cards: Option<String>,
    pub graph: Option<String>,
}

/// 默认：所有阶段使用同一模型（向后兼容）
impl Default for StageModelConfig {
    fn default() -> Self {
        Self {
            summary: None,
            entities: None,
            cards: None,
            graph: None,
        }
    }
}

impl StageModelConfig {
    /// 从环境变量加载阶段模型配置
    /// LLM_MODEL_SUMMARY / LLM_MODEL_ENTITIES / LLM_MODEL_CARDS / LLM_MODEL_GRAPH
    pub fn from_env() -> Self {
        Self {
            summary: std::env::var("LLM_MODEL_SUMMARY").ok(),
            entities: std::env::var("LLM_MODEL_ENTITIES").ok(),
            cards: std::env::var("LLM_MODEL_CARDS").ok(),
            graph: std::env::var("LLM_MODEL_GRAPH").ok(),
        }
    }

    /// 获取指定阶段的模型，未配置返回 None
    pub fn model_for_stage(&self, stage: &str) -> Option<&String> {
        match stage {
            "summary" => self.summary.as_ref(),
            "entities" => self.entities.as_ref(),
            "cards" => self.cards.as_ref(),
            "graph" => self.graph.as_ref(),
            _ => None,
        }
    }
}

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

// 已删除：ProviderConfig 和 get_provider_config
// 所有配置强制从 scan_credentials() / .cardnote/providers.json 读取
// 禁止从 ProviderRegistry 获取硬编码默认值作为运行时 fallback

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

    // 保存到 .cardnote/providers.json
    save_to_providers_json(cred)?;

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

/// 保存配置到 .cardnote/providers.json（新格式）
fn save_to_providers_json(cred: &ProviderCredential) -> Result<()> {
    let config_dir = std::path::PathBuf::from(".cardnote");
    std::fs::create_dir_all(&config_dir).map_err(AppError::Io)?;

    let path = config_dir.join("providers.json");

    // 读取已有内容（如果有）
    let mut existing: serde_json::Map<String, serde_json::Value> = if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        serde_json::Map::new()
    };

    let mut provider_block = serde_json::Map::new();
    provider_block.insert("api_key".to_string(), serde_json::Value::String(cred.api_key.clone()));
    if let Some(ref url) = cred.base_url {
        provider_block.insert("base_url".to_string(), serde_json::Value::String(url.clone()));
    }
    if let Some(ref model) = cred.default_model {
        provider_block.insert("model".to_string(), serde_json::Value::String(model.clone()));
    }

    existing.insert(cred.provider_id.clone(), serde_json::Value::Object(provider_block));

    let content = serde_json::to_string_pretty(&existing)
        .map_err(|e| AppError::Config(format!("配置序列化失败: {}", e)))?;
    std::fs::write(&path, content).map_err(AppError::Io)?;

    // [C4] 设置文件权限为 600，防止多用户系统下 API key 泄露
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&path, permissions).map_err(AppError::Io)?;
    }

    println!(
        "  💾 配置已保存到: {}\n",
        path.display().to_string().bright_black()
    );
    Ok(())
}

/// 快速配置检查 — 用于 doctor 命令
/// 优先 .cardnote/providers.json，回退 .env，最后交互式配置
pub async fn quick_config_check() -> Result<(String, String)> {
    // 1. 优先扫描 .cardnote/providers.json（新格式）
    let credentials = scan_credentials();
    if !credentials.is_empty() {
        let first_id = credentials.keys().next().unwrap().clone();
        let cred = credentials.get(&first_id).unwrap();
        return Ok((cred.api_key.clone(), first_id));
    }

    // 2. 回退到 .env 或环境变量（旧格式兼容）
    if let Some(config) = AppConfig::from_env()
        && !config.api_key.is_empty()
        && config.api_key.len() > 10
    {
        return Ok((config.api_key, config.provider));
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

// ═══════════════════════════════════════════════════════
//  书籍配置（运行时加载，无需重新编译）
// ═══════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// 书籍配置（从 .cardnote/books.json 加载）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BookConfig {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub author: Option<String>,
}

/// 加载书籍配置，运行时从 .cardnote/books.json 读取
/// 文件不存在或格式错误时返回默认配置
pub fn load_books_config() -> Vec<BookConfig> {
    let config_path = std::path::Path::new(".cardnote/books.json");
    if config_path.exists()
        && let Ok(content) = std::fs::read_to_string(config_path)
        && let Ok(books) = serde_json::from_str::<Vec<BookConfig>>(&content)
        && !books.is_empty()
    {
        return books;
    }

    // 默认配置
    vec![
        BookConfig {
            name: "人生模式".to_string(),
            aliases: vec!["人生模式".to_string()],
            author: Some("阳志平".to_string()),
        },
        BookConfig {
            name: "聪明的阅读者".to_string(),
            aliases: vec!["聪明的阅读者".to_string(), "阅读者".to_string()],
            author: Some("阳志平".to_string()),
        },
    ]
}

/// 获取所有已知书名（用于 ref 格式修复）
pub fn known_book_names() -> Vec<String> {
    load_books_config()
        .iter()
        .flat_map(|b| {
            let mut names = b.aliases.clone();
            names.push(b.name.clone());
            names
        })
        .collect()
}

// ═══════════════════════════════════════════════════════
//  密度标记词配置（运行时从 .cardnote/density_markers.toml 加载）
// ═══════════════════════════════════════════════════════

/// 信息密度标记词分类（权重用于 compute_info_density）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DensityMarkers {
    /// 权重 2.0：学术/专业术语
    #[serde(default)]
    pub academic_terms: Vec<String>,
    /// 权重 1.5：引用/来源标记
    #[serde(default)]
    pub citation_terms: Vec<String>,
    /// 权重 1.5：量化/数据标记
    #[serde(default)]
    pub quantifiers: Vec<String>,
    /// 权重 1.0：逻辑连接词
    #[serde(default)]
    pub logic_connectors: Vec<String>,
    /// 权重 0.5：结构化标记
    #[serde(default)]
    pub structure_markers: Vec<String>,
}

impl Default for DensityMarkers {
    fn default() -> Self {
        Self {
            academic_terms: vec![
                "研究发现".into(),
                "研究表明".into(),
                "实验证明".into(),
                "实验表明".into(),
                "理论".into(),
                "模型".into(),
                "框架".into(),
                "机制".into(),
                "原理".into(),
                "规律".into(),
                "概念".into(),
                "定义".into(),
                "术语".into(),
                "范式".into(),
                "假设".into(),
                "推论".into(),
                "认知".into(),
                "心理".into(),
                "神经".into(),
                "行为".into(),
                "情绪".into(),
                "动机".into(),
                "结构".into(),
                "系统".into(),
                "模式".into(),
                "流程".into(),
                "算法".into(),
                "函数".into(),
            ],
            citation_terms: vec![
                "提出".into(),
                "指出".into(),
                "认为".into(),
                "主张".into(),
                "强调".into(),
                "总结".into(),
                "引用".into(),
                "借鉴".into(),
                "参考".into(),
                "依据".into(),
                "根据".into(),
                "基于".into(),
            ],
            quantifiers: vec![
                "数据".into(),
                "证据".into(),
                "统计".into(),
                "调查".into(),
                "百分比".into(),
                "比例".into(),
                "数量".into(),
                "数值".into(),
                "指标".into(),
                "维度".into(),
                "程度".into(),
                "水平".into(),
                "大约".into(),
                "约".into(),
                "超过".into(),
                "低于".into(),
                "达到".into(),
                "增至".into(),
            ],
            logic_connectors: vec![
                "比如".into(),
                "例如".into(),
                "如".into(),
                "像".into(),
                "譬如".into(),
                "首先".into(),
                "其次".into(),
                "再次".into(),
                "最后".into(),
                "第一".into(),
                "第二".into(),
                "第三".into(),
                "因此".into(),
                "所以".into(),
                "因而".into(),
                "从而".into(),
                "于是".into(),
                "然而".into(),
                "但是".into(),
                "不过".into(),
                "却".into(),
                "而".into(),
                "反而".into(),
                "虽然".into(),
                "尽管".into(),
                "即使".into(),
                "纵然".into(),
                "如果".into(),
                "假设".into(),
                "若".into(),
                "只要".into(),
                "只有".into(),
                "那么".into(),
                "则".into(),
                "不仅".into(),
                "不但".into(),
                "而且".into(),
                "并且".into(),
                "同时".into(),
                "此外".into(),
                "另外".into(),
                "因为".into(),
                "由于".into(),
                "鉴于".into(),
                "考虑到".into(),
                "不同于".into(),
                "相较于".into(),
                "相比".into(),
                "相对".into(),
                "相反".into(),
                "反之".into(),
                "分为".into(),
                "包括".into(),
                "涵盖".into(),
                "包含".into(),
                "涉及".into(),
                "关于".into(),
                "通过".into(),
                "凭借".into(),
                "利用".into(),
                "采用".into(),
                "运用".into(),
                "使用".into(),
                "导致".into(),
                "造成".into(),
                "引起".into(),
                "引发".into(),
                "产生".into(),
                "带来".into(),
                "影响".into(),
                "作用".into(),
                "效果".into(),
                "结果".into(),
                "后果".into(),
                "成果".into(),
                "区别".into(),
                "差异".into(),
                "区分".into(),
                "辨别".into(),
                "识别".into(),
                "比较".into(),
                "对比".into(),
                "对照".into(),
                "类比".into(),
                "关键".into(),
                "核心".into(),
                "本质".into(),
                "实质".into(),
                "根本".into(),
                "重点".into(),
                "要点".into(),
                "原因".into(),
                "理由".into(),
                "根源".into(),
                "由来".into(),
                "起因".into(),
                "目的".into(),
                "目标".into(),
                "意图".into(),
                "旨在".into(),
                "为了".into(),
                "意义".into(),
                "价值".into(),
                "重要性".into(),
                "作用".into(),
                "方法".into(),
                "方式".into(),
                "途径".into(),
                "手段".into(),
                "策略".into(),
                "技巧".into(),
                "步骤".into(),
                "分析".into(),
                "解析".into(),
                "剖析".into(),
                "解读".into(),
                "阐释".into(),
                "阐明".into(),
                "论述".into(),
                "论证".into(),
                "总结".into(),
                "归纳".into(),
                "概括".into(),
                "综述".into(),
                "回顾".into(),
                "梳理".into(),
                "具体".into(),
                "详细".into(),
                "明确".into(),
                "清晰".into(),
                "确切".into(),
                "明确".into(),
                "实例".into(),
                "案例".into(),
                "事例".into(),
                "例子".into(),
                "样板".into(),
                "典型".into(),
            ],
            structure_markers: vec![
                "：".into(),
                ":".into(),
                "1.".into(),
                "2.".into(),
                "3.".into(),
                "4.".into(),
                "5.".into(),
                "一、".into(),
                "二、".into(),
                "三、".into(),
                "四、".into(),
                "五、".into(),
                "（1）".into(),
                "（2）".into(),
                "（3）".into(),
                "①".into(),
                "②".into(),
                "③".into(),
            ],
        }
    }
}

static DENSITY_MARKERS: std::sync::LazyLock<DensityMarkers> =
    std::sync::LazyLock::new(|| load_density_markers().unwrap_or_default());

fn load_density_markers() -> std::result::Result<DensityMarkers, String> {
    let path = std::path::Path::new(".cardnote/density_markers.toml");
    if path.exists() {
        let content = std::fs::read_to_string(path).map_err(|e| format!("读取失败: {}", e))?;
        toml::from_str(&content).map_err(|e| format!("解析失败: {}", e))
    } else {
        Ok(DensityMarkers::default())
    }
}

pub fn density_markers() -> &'static DensityMarkers {
    &DENSITY_MARKERS
}

/// 确保书名已在 .cardnote/books.json 中注册（不存在则追加）
pub fn ensure_book_registered(book_name: &str) {
    if book_name.is_empty() || book_name == "未命名" {
        return;
    }
    let mut books = load_books_config();
    let already_exists = books
        .iter()
        .any(|b| b.name == book_name || b.aliases.contains(&book_name.to_string()));
    if !already_exists {
        books.push(BookConfig {
            name: book_name.to_string(),
            aliases: vec![book_name.to_string()],
            author: None,
        });
        // 写回文件
        if let Ok(json) = serde_json::to_string_pretty(&books) {
            let _ = std::fs::create_dir_all(".cardnote");
            let _ = std::fs::write(".cardnote/books.json", json);
        }
    }
}

/// 读取 RPM 限流配置（默认 30 RPM，可通过环境变量覆盖）
pub fn max_rpm() -> Option<u32> {
    std::env::var("CARDNOTE_MAX_RPM")
        .ok()
        .and_then(|s| s.parse().ok())
        .or(Some(30)) // 默认 30 RPM，避免 429
}

/// 是否禁用 extract-then-assign 策略（某些模型不兼容）
/// 设置 CARDC_DISABLE_EXTRACT_ASSIGN=1 跳过必然失败的 extract+assign 两步，
/// 直接走 legacy 分类型生成，节省 2 次 LLM 调用/chunk。
pub fn disable_extract_assign() -> bool {
    std::env::var("CARDC_DISABLE_EXTRACT_ASSIGN")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false)
}
