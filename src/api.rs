use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use colored::Colorize;
use reqwest::Client;
use serde_json::Value;

use crate::config::get_provider_config;
use crate::error::{AppError, Result};
use crate::models::{LlmMessage, LlmRequest, LlmUsage, ResponseFormat};
use crate::providers::ProviderRegistry;
use crate::rate_limiter::RateLimiter;

/// 默认请求超时(秒)
const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// LLM 客户端（支持模型自动回退 + Token 用量追踪 + 多协议 JSON + RPM 限流）
#[derive(Clone, Debug)]
pub struct LlmClient {
    client: Client,
    api_key: String,
    base_url: String,
    pub model: String,
    /// 提供商 ID（用于区分协议类型：openai / anthropic / gemini / ...）
    provider_id: String,
    /// 同一提供商中支持 JSON mode 的备用模型列表
    fallback_models: Vec<String>,
    /// 当前使用的模型索引（0=主模型，1+=fallback），用 Arc<AtomicUsize> 支持 Clone + &self 调用
    /// [C1] 改用 AtomicUsize 避免 async 上下文中阻塞 tokio worker
    current_model_idx: Arc<AtomicUsize>,
    /// 累积的 LLM 调用用量统计
    usage_log: Arc<tokio::sync::Mutex<Vec<LlmUsage>>>,
    /// RPM 限流器（未配置 CARDNOTE_MAX_RPM 时为 None）
    rate_limiter: Option<Arc<tokio::sync::Mutex<RateLimiter>>>,
}

impl LlmClient {
    pub fn new(
        api_key: String,
        base_url: String,
        model: String,
        provider_id: String,
        fallback_models: Vec<String>,
    ) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| AppError::Api(format!("HTTP 客户端构建失败: {}", e)))?;
        let rate_limiter = crate::config::max_rpm()
            .map(|rpm| Arc::new(tokio::sync::Mutex::new(RateLimiter::new(rpm))));

        Ok(Self {
            client,
            api_key,
            base_url,
            model,
            provider_id,
            fallback_models,
            current_model_idx: Arc::new(AtomicUsize::new(0)),
            usage_log: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            rate_limiter,
        })
    }

    /// 获取当前实际使用的模型 ID
    fn current_model(&self) -> String {
        let idx = self.current_model_idx.load(Ordering::Relaxed);
        if idx == 0 {
            self.model.clone()
        } else {
            self.fallback_models
                .get(idx - 1)
                .cloned()
                .unwrap_or_else(|| self.model.clone())
        }
    }

    /// 切换到下一个可用模型，返回是否切换成功
    fn switch_to_next_model(&self) -> bool {
        let idx = self.current_model_idx.load(Ordering::Relaxed);
        if idx <= self.fallback_models.len() {
            let new_idx = idx + 1;
            self.current_model_idx.store(new_idx, Ordering::Relaxed);
            if new_idx <= self.fallback_models.len() {
                let new_model = if new_idx == 0 {
                    self.model.clone()
                } else {
                    self.fallback_models
                        .get(new_idx - 1)
                        .cloned()
                        .unwrap_or_else(|| self.model.clone())
                };
                println!(
                    "  🔄 模型不支持 JSON mode，自动切换到: {}",
                    new_model.bright_cyan()
                );
                return true;
            }
        }
        false
    }

    /// 重置到主模型（每次新请求前调用）
    fn reset_model(&self) {
        self.current_model_idx.store(0, Ordering::Relaxed);
    }

    /// 创建使用指定模型的临时 client（共享 HTTP 连接池、用量统计和限流器）
    pub fn with_model(&self, model: &str) -> Self {
        Self {
            client: self.client.clone(),
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            model: model.to_string(),
            provider_id: self.provider_id.clone(),
            fallback_models: self.fallback_models.clone(),
            current_model_idx: Arc::new(AtomicUsize::new(0)),
            usage_log: self.usage_log.clone(),
            rate_limiter: self.rate_limiter.clone(),
        }
    }

    /// 发送聊天请求，返回原始响应 JSON
    pub async fn chat(&self, messages: Vec<LlmMessage>, max_tokens: Option<u32>) -> Result<Value> {
        let request = LlmRequest {
            model: self.model.clone(),
            messages,
            max_tokens,
            response_format: None,
        };

        self.send_request(request).await
    }

    /// 发送聊天请求，要求 JSON 格式输出（含重试机制 + 模型自动回退）
    pub async fn chat_json(
        &self,
        messages: Vec<LlmMessage>,
        max_tokens: Option<u32>,
    ) -> Result<Value> {
        if messages.is_empty() {
            return Err(AppError::Api("消息列表不能为空".to_string()));
        }

        self.reset_model();

        // 尝试主模型 + 所有 fallback 模型
        loop {
            let model = self.current_model();
            let result = self
                .try_chat_json_with_model(&model, messages.clone(), max_tokens)
                .await;

            match result {
                Ok(json) => return Ok(json),
                Err(e) => {
                    let err_str = e.to_string();
                    // 检测 JSON mode 不支持的典型错误
                    let is_json_unsupported = err_str.contains("json_object")
                        && (err_str.contains("not supported")
                            || err_str.contains("not valid")
                            || err_str.contains("invalid"));

                    if is_json_unsupported && self.switch_to_next_model() {
                        continue;
                    }

                    // 不是 JSON mode 问题，或者没有更多 fallback 模型了
                    return Err(e);
                }
            }
        }
    }

    /// 用指定模型尝试 JSON 请求（内部方法）
    async fn try_chat_json_with_model(
        &self,
        model: &str,
        messages: Vec<LlmMessage>,
        max_tokens: Option<u32>,
    ) -> Result<Value> {
        let mut modified_messages = messages;
        if modified_messages[0].role == "system" {
            modified_messages[0].content +=
                "\n\n你必须严格按 JSON 格式输出，不要添加任何其他文字。";
        } else {
            modified_messages.insert(
                0,
                LlmMessage {
                    role: "system".to_string(),
                    content: "你必须严格按 JSON 格式输出，不要添加任何其他文字。".to_string(),
                },
            );
        }

        // 按 Provider 协议选择 JSON mode 策略
        // OpenAI 兼容协议：使用 response_format: json_object
        // Anthropic / Gemini / Cohere：仅依赖 system prompt 要求 JSON，不使用 response_format
        let use_json_object =
            !matches!(self.provider_id.as_str(), "anthropic" | "google" | "cohere");
        let request = LlmRequest {
            model: model.to_string(),
            messages: modified_messages,
            max_tokens,
            response_format: if use_json_object {
                Some(ResponseFormat::json_object())
            } else {
                None
            },
        };

        // 重试逻辑：最多 3 次，指数退避
        let mut last_error = None;
        let mut last_raw_content: Option<String> = None;
        for attempt in 1..=3 {
            match self.send_request(request.clone()).await {
                Ok(response) => match self.extract_content(&response) {
                    Ok(content) => {
                        last_raw_content = Some(content.clone());
                        match serde_json::from_str::<Value>(&content) {
                            Ok(json) => {
                                if json.as_object().is_some_and(|o| o.is_empty()) {
                                    let msg = "JSON 返回空对象 {}".to_string();
                                    eprintln!("    ⚠ JSON 验证失败 (尝试 {}/3): {}", attempt, msg);
                                    last_error = Some(AppError::JsonParse(msg));
                                } else if json.get("title").is_none()
                                    && json.as_array().is_some_and(|a| a.is_empty())
                                {
                                    let msg = "JSON 返回缺少 title 字段且为空数组".to_string();
                                    eprintln!("    ⚠ JSON 验证失败 (尝试 {}/3): {}", attempt, msg);
                                    last_error = Some(AppError::JsonParse(msg));
                                } else {
                                    return Ok(json);
                                }
                            }
                            Err(e) => {
                                let msg = format!("content JSON 解析失败: {}", e);
                                eprintln!("    ⚠ JSON 解析失败 (尝试 {}/3): {}", attempt, msg);
                                last_error = Some(AppError::JsonParse(msg));
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("    ⚠ 内容提取失败 (尝试 {}/3): {}", attempt, e);
                        last_error = Some(e);
                    }
                },
                Err(e) => {
                    eprintln!("    ⚠ 请求失败 (尝试 {}/3): {}", attempt, e);
                    last_error = Some(e);
                }
            }

            if attempt < 3 {
                let backoff = Duration::from_secs(2_u64.pow(attempt));
                eprintln!("    ⏳ 等待 {:?} 后重试...", backoff);
                tokio::time::sleep(backoff).await;
            }
        }

        // 所有重试失败后，尝试文本降级
        if let Some(raw) = last_raw_content {
            if let Some(degraded) = Self::degrade_json_to_text(&raw) {
                eprintln!("    ⚠ JSON 解析全部失败，已通过文本降级恢复");
                return Ok(degraded);
            }
        }
        Err(last_error.unwrap_or_else(|| AppError::Api("JSON 请求全部重试失败".to_string())))
    }

    /// JSON 解析失败后的文本降级：从原始文本中提取有效内容
    fn degrade_json_to_text(raw_text: &str) -> Option<Value> {
        // 策略A: 去除 markdown code fence 后重试 JSON
        if let Some(inner) = Self::extract_code_fence_json(raw_text)
            && let Ok(v) = serde_json::from_str::<Value>(&inner)
        {
            return Some(v);
        }
        // 策略B: 检测卡片格式（含 --- 分隔符和 标题 字段）
        if raw_text.contains("---") && raw_text.contains("标题") {
            let cards: Vec<Value> = raw_text
                .split("---")
                .filter(|s| !s.trim().is_empty())
                .filter_map(|block| {
                    if block.contains("标题") {
                        Some(serde_json::json!({
                            "raw_card": block.trim().to_string()
                        }))
                    } else {
                        None
                    }
                })
                .collect();
            if !cards.is_empty() {
                return Some(serde_json::json!({ "cards": cards }));
            }
        }
        // 策略C: 兜底 — 包装为 raw_text
        if !raw_text.trim().is_empty() {
            return Some(serde_json::json!({ "raw_text": raw_text }));
        }
        None
    }

    /// 从文本中提取 markdown code fence 内的 JSON
    fn extract_code_fence_json(text: &str) -> Option<String> {
        let start_marker = "```json\n";
        let alt_marker = "```\n";
        // 优先找 ```json
        if let Some(pos) = text.find(start_marker) {
            let inner = &text[pos + start_marker.len()..];
            if let Some(end) = inner.find("\n```") {
                return Some(inner[..end].trim().to_string());
            }
        }
        // 回退到普通 ```
        if let Some(pos) = text.find(alt_marker) {
            let inner = &text[pos + alt_marker.len()..];
            if let Some(end) = inner.find("\n```") {
                let candidate = inner[..end].trim().to_string();
                // 只当内容看起来像 JSON 时才提取
                if candidate.starts_with('{') || candidate.starts_with('[') {
                    return Some(candidate);
                }
            }
        }
        None
    }

    /// 从响应中提取内容文本
    pub fn extract_content(&self, response: &Value) -> Result<String> {
        // OpenAI 格式: choices[0].message.content
        if let Some(content) = response
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
        {
            return Ok(content.to_string());
        }

        // 某些 API 可能直接返回 content
        if let Some(content) = response.get("content").and_then(|c| c.as_str()) {
            return Ok(content.to_string());
        }

        Err(AppError::Api("无法从响应中提取内容".to_string()))
    }

    /// 从响应中提取 Token 用量（兼容 OpenAI/DeepSeek 和 Anthropic 格式）
    pub fn extract_usage(&self, response: &Value, latency_ms: u64) -> LlmUsage {
        let usage = response.get("usage");
        let mut prompt_tokens = 0u32;
        let mut completion_tokens = 0u32;
        let mut total_tokens = 0u32;
        let mut cached_tokens = 0u32;
        let mut cache_creation_tokens = 0u32;

        if let Some(u) = usage {
            // OpenAI / DeepSeek 格式: prompt_tokens / completion_tokens / total_tokens
            prompt_tokens = u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            completion_tokens = u
                .get("completion_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            total_tokens = u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            // Anthropic 格式: input_tokens / output_tokens
            if prompt_tokens == 0 {
                prompt_tokens = u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                completion_tokens =
                    u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                total_tokens = prompt_tokens + completion_tokens;
            }
            // Anthropic 缓存字段
            cached_tokens = u
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            cache_creation_tokens = u
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
        }

        LlmUsage {
            prompt_tokens,
            completion_tokens,
            total_tokens,
            cached_tokens,
            cache_creation_tokens,
            model: self.current_model(),
            latency_ms,
        }
    }

    /// 记录一次调用用量（内存 + 文件持久化）
    pub async fn record_usage(&self, usage: LlmUsage) {
        let mut log = self.usage_log.lock().await;
        log.push(usage.clone());
        drop(log); // 显式释放锁，避免持有锁期间做 IO

        // 持久化到文件（追加模式，JSON Lines）
        if let Ok(json) = serde_json::to_string(&usage) {
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(".cardnote/usage.log")
                .and_then(|mut f| {
                    use std::io::Write;
                    writeln!(f, "{}", json)
                });
        }
    }

    /// 获取累积的用量报告
    pub async fn usage_report(&self) -> String {
        let log = self.usage_log.lock().await;
        LlmUsage::format_report(&log)
    }

    /// 获取累计 token 数（供编译记录用）
    pub async fn usage_totals(&self) -> (u32, u32) {
        let log = self.usage_log.lock().await;
        let prompt: u32 = log.iter().map(|u| u.prompt_tokens).sum();
        let completion: u32 = log.iter().map(|u| u.completion_tokens).sum();
        (prompt, completion)
    }

    /// 清空用量日志
    pub async fn clear_usage(&self) {
        let mut log = self.usage_log.lock().await;
        log.clear();
    }

    /// 发送 HTTP 请求（内部自动记录 Token 用量和延迟）
    async fn send_request(&self, request: LlmRequest) -> Result<Value> {
        // RPM 限流（如果配置了 CARDNOTE_MAX_RPM）
        if let Some(ref limiter) = self.rate_limiter {
            limiter.lock().await.acquire().await;
        }

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let start = std::time::Instant::now();
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::Api(format!("请求发送失败: {}", e)))?;

        let status = response.status();
        let body = response
            .json::<Value>()
            .await
            .map_err(|e| AppError::Api(format!("响应解析失败: {}", e)))?;

        let latency_ms = start.elapsed().as_millis() as u64;
        let usage = self.extract_usage(&body, latency_ms);
        self.record_usage(usage).await;

        if !status.is_success() {
            let error_msg = body
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("未知 API 错误");
            return Err(AppError::Api(format!(
                "API 错误 ({}): {}",
                status, error_msg
            )));
        }

        Ok(body)
    }
}

/// 根据提供商配置创建 LLM 客户端（自动查找 JSON-capable fallback 模型）
pub fn create_client(
    provider: &str,
    api_key: &str,
    model: Option<String>,
    base_url: Option<String>,
) -> Result<LlmClient> {
    let effective_model = model
        .clone()
        .or_else(|| get_provider_config(provider).map(|c| c.default_model.to_string()))
        .unwrap_or_else(|| "unknown".to_string());

    let effective_base_url = base_url
        .or_else(|| get_provider_config(provider).map(|c| c.base_url.to_string()))
        .unwrap_or_else(|| "https://api.deepseek.com/v1".to_string());

    // [C5] 验证 base_url 格式，防止请求到恶意服务器
    if let Err(e) = reqwest::Url::parse(&effective_base_url) {
        return Err(AppError::Config(format!(
            "无效的 base_url '{}': {}",
            effective_base_url, e
        )));
    }

    // 自动查找同一提供商中支持 JSON mode 的 fallback 模型
    let registry = ProviderRegistry::new();
    let mut fallback_models = registry.find_json_capable_models(provider);

    // 如果主模型已经在 fallback 列表中，移除它（避免重复尝试）
    fallback_models.retain(|m| m != &effective_model);

    // 如果用户指定了模型且该模型不支持 JSON mode，给出提示
    if let Some(ref user_model) = model
        && !registry.supports_json_mode(provider, user_model)
        && !fallback_models.is_empty()
    {
        println!(
            "  ⚠ 模型 {} 不支持 JSON mode，已配置 fallback: {}",
            user_model.bright_yellow(),
            fallback_models.join(", ").bright_cyan()
        );
    }

    LlmClient::new(
        api_key.to_string(),
        effective_base_url,
        effective_model,
        provider.to_string(),
        fallback_models,
    )
}
