use serde_json::Value;

use crate::error::{AppError, Result};
use crate::models::LlmMessage;

/// 从 JSON Value 中提取数组
///
/// 优先从指定 key 获取，fallback 到顶层数组
pub fn extract_json_array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .or_else(|| value.as_array())
        .ok_or_else(|| AppError::JsonParse(format!("无法解析 {} 列表", key)))
}

/// 文本 LLM 回调 trait — 消除泛型签名重复
pub trait ChatFn: Send + Sync {
    fn call_chat(
        &self,
        messages: Vec<LlmMessage>,
        max_tokens: Option<u32>,
    ) -> impl std::future::Future<Output = Result<String>> + Send;
}

/// JSON LLM 回调 trait — 消除泛型签名重复
pub trait ChatJsonFn: Send + Sync {
    fn call_json(
        &self,
        messages: Vec<LlmMessage>,
        max_tokens: Option<u32>,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
}

/// 为引用类型自动转发，支持 &T 作为 ChatFn 使用
impl<T: ChatFn + ?Sized> ChatFn for &T {
    async fn call_chat(
        &self,
        messages: Vec<LlmMessage>,
        max_tokens: Option<u32>,
    ) -> Result<String> {
        (**self).call_chat(messages, max_tokens).await
    }
}

/// 为引用类型自动转发，支持 &T 作为 ChatJsonFn 使用
impl<T: ChatJsonFn + ?Sized> ChatJsonFn for &T {
    async fn call_json(&self, messages: Vec<LlmMessage>, max_tokens: Option<u32>) -> Result<Value> {
        (**self).call_json(messages, max_tokens).await
    }
}
