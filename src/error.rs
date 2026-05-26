use std::io;

/// 统一错误类型
#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("输入转换失败: {0}")]
    Conversion(String),

    #[error("文件不存在: {0}")]
    FileNotFound(String),

    #[error("编码错误: {0}")]
    Encoding(String),

    #[error("LLM API 错误: {0}")]
    Api(String),

    #[error("Prompt 模板加载失败: {0}")]
    PromptLoad(String),

    #[error("JSON 解析错误: {0}")]
    JsonParse(String),

    #[error("配置错误: {0}")]
    Config(String),

    #[error("超时: {0}")]
    Timeout(String),

    #[error("任务执行失败: {0}")]
    TaskPanic(String),

    #[error("验证失败: {0}")]
    Validation(String),

    #[error("IO 错误: {0}")]
    Io(#[from] io::Error),

    #[error("文件过大: {0}")]
    FileTooLarge(String),

    #[error("未知错误: {0}")]
    Unknown(String),
}

pub type Result<T> = std::result::Result<T, AppError>;
