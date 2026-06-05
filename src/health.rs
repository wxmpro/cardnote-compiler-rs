use std::time::Instant;

use colored::Colorize;

use crate::api::LlmClient;
use crate::models::LlmMessage;
use crate::providers::{ProviderCredential, ProviderRegistry, scan_credentials};

/// Provider 健康检测结果
#[derive(Debug, Clone)]
pub struct ProviderHealth {
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub latency_ms: u64,
    pub available: bool,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub error: Option<String>,
}

impl ProviderHealth {
    pub fn status_icon(&self) -> &'static str {
        if self.available { "✅" } else { "❌" }
    }
}

/// 检测单个 Provider 的连通性
async fn check_provider(cred: &ProviderCredential) -> Vec<ProviderHealth> {
    let registry = ProviderRegistry::new();
    let provider = registry.get(&cred.provider_id);
    let provider_name = provider
        .map(|p| p.name.clone())
        .unwrap_or_else(|| cred.provider_id.clone());

    // 确定要测试的模型：强制从配置读取，禁止 fallback 到硬编码
    let models_to_test: Vec<String> = match cred.default_model {
        Some(ref model) => vec![model.clone()],
        None => {
            eprintln!(
                "  ⚠ Provider '{}' 未配置 model，跳过健康检测",
                cred.provider_id
            );
            return vec![ProviderHealth {
                provider_id: cred.provider_id.clone(),
                provider_name: cred.provider_id.clone(),
                model: "未配置".to_string(),
                latency_ms: 0,
                available: false,
                prompt_tokens: 0,
                completion_tokens: 0,
                error: Some("未在 .cardnote/providers.json 中配置 model 字段".to_string()),
            }];
        }
    };

    let mut results = Vec::new();

    for model in models_to_test {
        let start = Instant::now();

        // 强制从配置读取 base_url，禁止 fallback 到硬编码
        let base_url = match cred.base_url.clone() {
            Some(url) => url,
            None => {
                results.push(ProviderHealth {
                    provider_id: cred.provider_id.clone(),
                    provider_name: provider_name.clone(),
                    model: model.clone(),
                    latency_ms: 0,
                    available: false,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    error: Some(format!(
                        "Provider '{}' 未配置 base_url，请在 .cardnote/providers.json 中添加",
                        cred.provider_id
                    )),
                });
                continue;
            }
        };

        let client = match LlmClient::new(
            cred.api_key.clone(),
            base_url,
            model.clone(),
            cred.provider_id.clone(),
            Vec::new(), // fallback_models，健康检测不需要
        ) {
            Ok(c) => c,
            Err(e) => {
                results.push(ProviderHealth {
                    provider_id: cred.provider_id.clone(),
                    provider_name: provider_name.clone(),
                    model: model.clone(),
                    latency_ms: 0,
                    available: false,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    error: Some(format!("客户端创建失败: {}", e)),
                });
                continue;
            }
        };

        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: "你好".to_string(),
        }];

        match client.chat(messages, Some(10)).await {
            Ok(response) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                let content = client.extract_content(&response).ok();
                let usage = client.extract_usage(&response, latency_ms);

                let available = content.map(|c| !c.is_empty()).unwrap_or(false);

                results.push(ProviderHealth {
                    provider_id: cred.provider_id.clone(),
                    provider_name: provider_name.clone(),
                    model: model.clone(),
                    latency_ms,
                    available,
                    prompt_tokens: usage.prompt_tokens,
                    completion_tokens: usage.completion_tokens,
                    error: if available {
                        None
                    } else {
                        Some("返回 200 但无内容".to_string())
                    },
                });
            }
            Err(e) => {
                results.push(ProviderHealth {
                    provider_id: cred.provider_id.clone(),
                    provider_name: provider_name.clone(),
                    model: model.clone(),
                    latency_ms: start.elapsed().as_millis() as u64,
                    available: false,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    error: Some(format!("{}", e)),
                });
            }
        }
    }

    results
}

/// 检测所有已配置 Provider 的连通性
pub async fn check_all_providers() -> Vec<ProviderHealth> {
    let credentials = scan_credentials();
    let mut all_results = Vec::new();

    for cred in credentials.values() {
        let mut results = check_provider(cred).await;
        all_results.append(&mut results);
    }

    // 按可用性（可用优先）然后按延迟排序
    all_results.sort_by(|a, b| match (a.available, b.available) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.latency_ms.cmp(&b.latency_ms),
    });

    all_results
}

/// 打印健康度报告
pub fn print_health_report(results: &[ProviderHealth]) {
    println!(
        "\n{}",
        "╔════════════════════════════════════════════════════════════╗".bright_cyan()
    );
    println!(
        "{}",
        "║           Provider 连通性检测报告                        ║".bright_cyan()
    );
    println!(
        "{}",
        "╠════════════════════════════════════════════════════════════╣".bright_cyan()
    );

    let available = results.iter().filter(|r| r.available).count();
    let total = results.len();

    println!(
        "║ {:<56}║",
        format!(
            "总计: {} 个模型 | ✅ 可用 {} | ❌ 失败 {}",
            total,
            available,
            total - available
        )
    );
    println!(
        "{}",
        "╠════════════════════════════════════════════════════════════╣".bright_cyan()
    );

    for result in results {
        let status = if result.available {
            format!("{} {}ms", "可用".green(), result.latency_ms)
        } else {
            format!(
                "{} — {}",
                "失败".red(),
                result.error.as_deref().unwrap_or("未知")
            )
        };
        let line = format!(
            "{} {:<12} {:<20} {}",
            result.status_icon(),
            result.provider_name,
            result.model.chars().take(18).collect::<String>(),
            status
        );
        println!("║ {:<56}║", line.chars().take(56).collect::<String>());
    }

    println!(
        "{}",
        "╚════════════════════════════════════════════════════════════╝".bright_cyan()
    );
}

/// 选择最佳 Provider（可用且延迟最低）
pub fn select_best(results: &[ProviderHealth]) -> Option<(String, String, String)> {
    results.iter().find(|r| r.available).map(|r| {
        (
            r.provider_id.clone(),
            r.model.clone(),
            format!("{} / {}", r.provider_name, r.model),
        )
    })
}
