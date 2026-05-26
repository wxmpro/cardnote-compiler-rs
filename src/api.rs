use std::sync::{Arc, Mutex};
use std::time::Duration;

use colored::Colorize;
use reqwest::Client;
use serde_json::Value;

use crate::config::get_provider_config;
use crate::error::{AppError, Result};
use crate::models::{LlmMessage, LlmRequest, ResponseFormat};
use crate::providers::ProviderRegistry;

/// 默认请求超时(秒)
const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// LLM 客户端（支持模型自动回退）
#[derive(Debug, Clone)]
pub struct LlmClient {
    client: Client,
    api_key: String,
    base_url: String,
    pub model: String,
    /// 同一提供商中支持 JSON mode 的备用模型列表
    fallback_models: Vec<String>,
    /// 当前使用的模型索引（0=主模型，1+=fallback），用 Arc<Mutex> 支持 Clone + &self 调用
    current_model_idx: Arc<Mutex<usize>>,
}

impl LlmClient {
    pub fn new(
        api_key: String,
        base_url: String,
        model: String,
        fallback_models: Vec<String>,
    ) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| AppError::Api(format!("HTTP 客户端构建失败: {}", e)))?;
        Ok(Self {
            client,
            api_key,
            base_url,
            model,
            fallback_models,
            current_model_idx: Arc::new(Mutex::new(0)),
        })
    }

    /// 获取当前实际使用的模型 ID
    fn current_model(&self) -> String {
        let idx = *self.current_model_idx.lock().unwrap();
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
        let mut idx = self.current_model_idx.lock().unwrap();
        if *idx <= self.fallback_models.len() {
            *idx += 1;
            if *idx <= self.fallback_models.len() {
                let new_model = if *idx == 0 {
                    self.model.clone()
                } else {
                    self.fallback_models
                        .get(*idx - 1)
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
        *self.current_model_idx.lock().unwrap() = 0;
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

        let request = LlmRequest {
            model: model.to_string(),
            messages: modified_messages,
            max_tokens,
            response_format: Some(ResponseFormat::json_object()),
        };

        // 重试逻辑：最多 3 次，指数退避
        let mut last_error = None;
        for attempt in 1..=3 {
            match self.send_request(request.clone()).await {
                Ok(response) => match self.extract_content(&response) {
                    Ok(content) => match serde_json::from_str::<Value>(&content) {
                        Ok(json) => return Ok(json),
                        Err(e) => {
                            let msg = format!("content JSON 解析失败: {}", e);
                            eprintln!("    ⚠ JSON 解析失败 (尝试 {}/3): {}", attempt, msg);
                            last_error = Some(AppError::JsonParse(msg));
                        }
                    },
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

        Err(last_error.unwrap_or_else(|| AppError::Api("JSON 请求全部重试失败".to_string())))
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

    /// 发送 HTTP 请求
    async fn send_request(&self, request: LlmRequest) -> Result<Value> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

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

    // 自动查找同一提供商中支持 JSON mode 的 fallback 模型
    let registry = ProviderRegistry::new();
    let mut fallback_models = registry.find_json_capable_models(provider);

    // 如果主模型已经在 fallback 列表中，移除它（避免重复尝试）
    fallback_models.retain(|m| m != &effective_model);

    // 如果用户指定了模型且该模型不支持 JSON mode，给出提示
    if let Some(ref user_model) = model {
        if !registry.supports_json_mode(provider, user_model) && !fallback_models.is_empty() {
            println!(
                "  ⚠ 模型 {} 不支持 JSON mode，已配置 fallback: {}",
                user_model.bright_yellow(),
                fallback_models.join(", ").bright_cyan()
            );
        }
    }

    LlmClient::new(
        api_key.to_string(),
        effective_base_url,
        effective_model,
        fallback_models,
    )
}
