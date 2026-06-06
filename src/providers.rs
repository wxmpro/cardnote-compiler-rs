use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// 模型信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub context_length: usize,
    pub max_output_tokens: usize,
    pub supports_json_mode: bool,
    pub supports_vision: bool,
    pub description: String,
}

/// 提供商 API 协议
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderProtocol {
    OpenAiCompatible,
    Anthropic,
    Gemini,
    Cohere,
}

impl ProviderProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderProtocol::OpenAiCompatible => "OpenAI-compatible",
            ProviderProtocol::Anthropic => "Anthropic Messages",
            ProviderProtocol::Gemini => "Gemini Generative Language",
            ProviderProtocol::Cohere => "Cohere Chat",
        }
    }

    pub fn is_supported(&self) -> bool {
        matches!(self, ProviderProtocol::OpenAiCompatible)
    }
}

/// LLM 提供商
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub default_base_url: String,
    pub api_key_env_var: String,
    pub api_key_pattern: String, // 用于验证 key 格式，如 "sk-"
    pub models: Vec<ModelInfo>,
    pub requires_api_key: bool,
    pub protocol: ProviderProtocol,
    pub config_file_paths: Vec<String>, // 常见的配置文件路径
    pub description: String,
}

impl Provider {
    /// 获取默认模型
    pub fn default_model(&self) -> Option<&ModelInfo> {
        self.models.first()
    }

    /// 根据 ID 查找模型
    pub fn find_model(&self, model_id: &str) -> Option<&ModelInfo> {
        self.models
            .iter()
            .find(|m| m.id == model_id || m.name == model_id)
    }

    /// 验证 API key 格式
    pub fn validate_key_format(&self, key: &str) -> bool {
        if !self.requires_api_key {
            return true;
        }
        key.starts_with(&self.api_key_pattern)
    }
}

/// 已配置的提供商凭据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCredential {
    pub provider_id: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
    pub source: CredentialSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CredentialSource {
    EnvVar,
    ConfigFile(String),
    UserInput,
    AutoDetected,
}

/// 提供商注册表
pub struct ProviderRegistry {
    providers: HashMap<String, Provider>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            providers: HashMap::new(),
        };
        registry.register_all();
        registry
    }

    pub fn get(&self, id: &str) -> Option<&Provider> {
        self.providers.get(id)
    }

    pub fn list_all(&self) -> Vec<&Provider> {
        self.providers.values().collect()
    }

    pub fn find_by_alias(&self, alias: &str) -> Option<&Provider> {
        let alias_lower = alias.to_lowercase();
        self.providers.values().find(|p| {
            p.id.to_lowercase() == alias_lower
                || p.name.to_lowercase() == alias_lower
                || p.aliases.iter().any(|a| a.to_lowercase() == alias_lower)
        })
    }

    /// 获取指定提供商所有支持 JSON mode 的模型（按优先级排序）
    pub fn find_json_capable_models(&self, provider_id: &str) -> Vec<String> {
        self.providers
            .get(provider_id)
            .map(|p| {
                p.models
                    .iter()
                    .filter(|m| m.supports_json_mode)
                    .map(|m| m.id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 检查指定模型是否支持 JSON mode
    pub fn supports_json_mode(&self, provider_id: &str, model_id: &str) -> bool {
        self.providers
            .get(provider_id)
            .and_then(|p| p.find_model(model_id))
            .map(|m| m.supports_json_mode)
            .unwrap_or(false)
    }

    /// 查找指定提供商和模型的上下文长度
    pub fn find_model_context_length(&self, provider_id: &str, model_id: &str) -> Option<usize> {
        self.providers
            .get(provider_id)
            .and_then(|p| p.find_model(model_id))
            .map(|m| m.context_length)
    }

    /// 查找指定提供商和模型的最大输出 tokens
    pub fn find_model_max_output_tokens(&self, provider_id: &str, model_id: &str) -> Option<usize> {
        self.providers
            .get(provider_id)
            .and_then(|p| p.find_model(model_id))
            .map(|m| m.max_output_tokens)
    }

    fn register(&mut self, provider: Provider) {
        self.providers.insert(provider.id.clone(), provider);
    }

    /// [H7] 优先从外部配置文件加载提供商，避免新增需重编译
    ///
    /// 加载策略：
    /// 1. 先加载默认配置（从 ~/.config/cardnote/providers.default.json 或内置硬编码）
    /// 2. 再加载用户覆盖配置（从 ~/.config/cardnote/providers.json 或 ./providers.json）
    /// 3. 用户配置中的 Provider 会覆盖默认配置中的同名 Provider
    fn register_all(&mut self) {
        // 第一步：加载默认配置
        let default_loaded = self.load_default_config();
        if !default_loaded {
            self.register_builtin();
        }

        // 第二步：加载用户覆盖配置
        self.load_user_override_config();
    }

    /// 加载默认 Provider 配置
    /// 搜索路径：~/.config/cardnote/providers.default.json
    /// 若不存在，导出内置配置到该路径后加载
    fn load_default_config(&mut self) -> bool {
        let default_path =
            shellexpand::tilde("~/.config/cardnote/providers.default.json").to_string();
        let path = std::path::Path::new(&default_path);

        // 若默认配置文件不存在，导出内置配置
        if !path.exists() {
            let config_dir = path.parent().unwrap();
            if std::fs::create_dir_all(config_dir).is_ok() {
                let default_json = Self::export_builtin_config();
                if std::fs::write(path, default_json).is_ok() {
                    eprintln!("  💾 默认 Provider 配置已导出到: {}", default_path);
                }
            }
        }

        if let Ok(content) = std::fs::read_to_string(path)
            && let Ok(providers) = serde_json::from_str::<Vec<Provider>>(&content)
        {
            for p in providers {
                self.register(p);
            }
            return !self.providers.is_empty();
        }
        false
    }

    /// 加载用户覆盖配置
    /// 搜索路径：~/.config/cardnote/providers.json → ./providers.json
    /// 用户配置中的 Provider 会覆盖默认配置中的同名 Provider
    fn load_user_override_config(&mut self) {
        let paths = [
            shellexpand::tilde("~/.config/cardnote/providers.json").to_string(),
            "providers.json".to_string(),
        ];
        for path in &paths {
            if let Ok(content) = std::fs::read_to_string(path)
                && let Ok(providers) = serde_json::from_str::<Vec<Provider>>(&content)
            {
                for p in providers {
                    self.register(p); // 覆盖同名 Provider
                }
                return;
            }
        }
    }

    /// 导出默认内置配置为 JSON 字符串
    pub fn export_builtin_config() -> String {
        let mut registry = Self {
            providers: HashMap::new(),
        };
        registry.register_builtin();
        let providers: Vec<_> = registry.providers.into_values().collect();
        serde_json::to_string_pretty(&providers)
            .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
    }

    /// 内置硬编码配置（fallback，当外部配置文件不存在时使用）
    fn register_builtin(&mut self) {
        // Anthropic (Claude)
        self.register(Provider {
            id: "anthropic".to_string(),
            name: "Anthropic".to_string(),
            aliases: vec!["claude".to_string(), "claude-code".to_string()],
            default_base_url: "https://api.anthropic.com/v1".to_string(),
            api_key_env_var: "ANTHROPIC_API_KEY".to_string(),
            api_key_pattern: "sk-ant-".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::Anthropic,
            config_file_paths: vec![
                "~/.claude/settings.json".to_string(),
                "~/.claude/settings.local.json".to_string(),
            ],
            description: "Anthropic Claude 系列模型".to_string(),
            models: vec![
                ModelInfo {
                    id: "claude-opus-4-7".to_string(),
                    name: "Claude Opus 4.7".to_string(),
                    context_length: 200_000,
                    max_output_tokens: 8192,
                    supports_json_mode: true,
                    supports_vision: true,
                    description: "最强推理能力，适合复杂分析".to_string(),
                },
                ModelInfo {
                    id: "claude-sonnet-4-6".to_string(),
                    name: "Claude Sonnet 4.6".to_string(),
                    context_length: 200_000,
                    max_output_tokens: 8192,
                    supports_json_mode: true,
                    supports_vision: true,
                    description: "平衡性能与速度".to_string(),
                },
                ModelInfo {
                    id: "claude-haiku-4-5".to_string(),
                    name: "Claude Haiku 4.5".to_string(),
                    context_length: 200_000,
                    max_output_tokens: 4096,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "快速响应，低成本".to_string(),
                },
            ],
        });

        // OpenAI
        self.register(Provider {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            aliases: vec!["codex".to_string(), "chatgpt".to_string()],
            default_base_url: "https://api.openai.com/v1".to_string(),
            api_key_env_var: "OPENAI_API_KEY".to_string(),
            api_key_pattern: "sk-".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::OpenAiCompatible,
            config_file_paths: vec!["~/.config/openai/config.json".to_string()],
            description: "OpenAI GPT 系列模型".to_string(),
            models: vec![
                ModelInfo {
                    id: "gpt-4o".to_string(),
                    name: "GPT-4o".to_string(),
                    context_length: 128_000,
                    max_output_tokens: 16384,
                    supports_json_mode: true,
                    supports_vision: true,
                    description: "多模态旗舰模型".to_string(),
                },
                ModelInfo {
                    id: "gpt-4".to_string(),
                    name: "GPT-4".to_string(),
                    context_length: 8192,
                    max_output_tokens: 4096,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "高可靠性文本模型".to_string(),
                },
                ModelInfo {
                    id: "gpt-3.5-turbo".to_string(),
                    name: "GPT-3.5 Turbo".to_string(),
                    context_length: 16_385,
                    max_output_tokens: 4096,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "快速经济型".to_string(),
                },
            ],
        });

        // DeepSeek
        self.register(Provider {
            id: "deepseek".to_string(),
            name: "DeepSeek".to_string(),
            aliases: vec!["deep-seek".to_string()],
            default_base_url: "https://api.deepseek.com/v1".to_string(),
            api_key_env_var: "DEEPSEEK_API_KEY".to_string(),
            api_key_pattern: "sk-".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::OpenAiCompatible,
            config_file_paths: vec!["~/.config/deepseek/config.json".to_string()],
            description: "DeepSeek 大模型".to_string(),
            models: vec![
                ModelInfo {
                    id: "deepseek-v4-pro".to_string(),
                    name: "DeepSeek V4 Pro".to_string(),
                    context_length: 1_000_000,
                    // 实测: api.deepseek.com 对 150K max_tokens + 大文档请求偶发返回空响应体
                    // 降至 100K 平衡生成容量与稳定性（配合 600s 超时）
                    max_output_tokens: 64_000,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "DeepSeek V4 Pro — 1M上下文/100K输出(实测稳定值)".to_string(),
                },
                ModelInfo {
                    id: "deepseek-v4-flash".to_string(),
                    name: "DeepSeek V4 Flash".to_string(),
                    context_length: 1_000_000,
                    max_output_tokens: 64_000,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "DeepSeek V4 Flash — 1M上下文/100K输出(实测稳定值)".to_string(),
                },
            ],
        });

        // Google Gemini
        self.register(Provider {
            id: "google".to_string(),
            name: "Google".to_string(),
            aliases: vec!["gemini".to_string()],
            default_base_url: "https://generativelanguage.googleapis.com/v1".to_string(),
            api_key_env_var: "GEMINI_API_KEY".to_string(),
            api_key_pattern: "AIza".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::Gemini,
            config_file_paths: vec![
                "~/.config/cardnote-compiler/providers.json".to_string(),
                ".cardnote/providers.json".to_string(),
            ],
            description: "Google Gemini 系列模型".to_string(),
            models: vec![
                ModelInfo {
                    id: "gemini-1.5-pro".to_string(),
                    name: "Gemini 1.5 Pro".to_string(),
                    context_length: 1_000_000,
                    max_output_tokens: 8192,
                    supports_json_mode: true,
                    supports_vision: true,
                    description: "超长上下文，多模态".to_string(),
                },
                ModelInfo {
                    id: "gemini-1.5-flash".to_string(),
                    name: "Gemini 1.5 Flash".to_string(),
                    context_length: 1_000_000,
                    max_output_tokens: 8192,
                    supports_json_mode: true,
                    supports_vision: true,
                    description: "快速经济型".to_string(),
                },
            ],
        });

        // Moonshot AI (Kimi)
        self.register(Provider {
            id: "moonshot".to_string(),
            name: "Moonshot AI".to_string(),
            aliases: vec!["kimi".to_string()],
            default_base_url: "https://api.moonshot.cn/v1".to_string(),
            api_key_env_var: "MOONSHOT_API_KEY".to_string(),
            api_key_pattern: "sk-".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::OpenAiCompatible,
            config_file_paths: vec![
                "~/.config/cardnote-compiler/providers.json".to_string(),
                ".cardnote/providers.json".to_string(),
            ],
            description: "Moonshot Kimi 大模型".to_string(),
            models: vec![
                ModelInfo {
                    id: "kimi-k2.6".to_string(),
                    name: "Kimi K2.6".to_string(),
                    context_length: 256_000,
                    max_output_tokens: 96_000,
                    supports_json_mode: true,
                    supports_vision: true,
                    description: "Kimi 2.6 — 256K上下文，1T参数MoE".to_string(),
                },
                ModelInfo {
                    id: "moonshot-v1-128k".to_string(),
                    name: "Kimi 128K".to_string(),
                    context_length: 128_000,
                    max_output_tokens: 4096,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "长上下文模型".to_string(),
                },
                ModelInfo {
                    id: "moonshot-v1-32k".to_string(),
                    name: "Kimi 32K".to_string(),
                    context_length: 32_000,
                    max_output_tokens: 4096,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "标准上下文".to_string(),
                },
            ],
        });

        // 阿里云通义千问
        self.register(Provider {
            id: "aliyun".to_string(),
            name: "阿里云".to_string(),
            aliases: vec![
                "qwen".to_string(),
                "tongyi".to_string(),
                "dashscope".to_string(),
            ],
            default_base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
            api_key_env_var: "DASHSCOPE_API_KEY".to_string(),
            api_key_pattern: "sk-".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::OpenAiCompatible,
            config_file_paths: vec![
                "~/.config/cardnote-compiler/providers.json".to_string(),
                ".cardnote/providers.json".to_string(),
            ],
            description: "阿里云通义千问系列".to_string(),
            models: vec![
                ModelInfo {
                    id: "qwen-max".to_string(),
                    name: "通义千问 Max".to_string(),
                    context_length: 32_000,
                    max_output_tokens: 8192,
                    supports_json_mode: true,
                    supports_vision: true,
                    description: "最强能力".to_string(),
                },
                ModelInfo {
                    id: "qwen-plus".to_string(),
                    name: "通义千问 Plus".to_string(),
                    context_length: 32_000,
                    max_output_tokens: 4096,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "平衡性能".to_string(),
                },
                ModelInfo {
                    id: "qwen-turbo".to_string(),
                    name: "通义千问 Turbo".to_string(),
                    context_length: 32_000,
                    max_output_tokens: 4096,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "快速经济".to_string(),
                },
            ],
        });

        // 智谱 AI (GLM)
        self.register(Provider {
            id: "zhipu".to_string(),
            name: "智谱 AI".to_string(),
            aliases: vec!["glm".to_string(), "chatglm".to_string()],
            default_base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
            api_key_env_var: "ZHIPU_API_KEY".to_string(),
            api_key_pattern: "".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::OpenAiCompatible,
            config_file_paths: vec![
                "~/.config/cardnote-compiler/providers.json".to_string(),
                ".cardnote/providers.json".to_string(),
            ],
            description: "智谱 GLM 系列模型".to_string(),
            models: vec![
                ModelInfo {
                    id: "glm-5.1".to_string(),
                    name: "GLM-5.1".to_string(),
                    context_length: 202_752,
                    max_output_tokens: 128_000,
                    supports_json_mode: true,
                    supports_vision: true,
                    description: "GLM 5.1 — 203K上下文，MoE架构".to_string(),
                },
                ModelInfo {
                    id: "glm-4".to_string(),
                    name: "GLM-4".to_string(),
                    context_length: 128_000,
                    max_output_tokens: 4096,
                    supports_json_mode: true,
                    supports_vision: true,
                    description: "旗舰模型".to_string(),
                },
                ModelInfo {
                    id: "glm-4-flash".to_string(),
                    name: "GLM-4-Flash".to_string(),
                    context_length: 128_000,
                    max_output_tokens: 4096,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "免费快速".to_string(),
                },
            ],
        });

        // MiniMax
        self.register(Provider {
            id: "minimax".to_string(),
            name: "MiniMax".to_string(),
            aliases: vec!["minimax".to_string()],
            default_base_url: "https://api.minimax.chat/v1".to_string(),
            api_key_env_var: "MINIMAX_API_KEY".to_string(),
            api_key_pattern: "sk-".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::OpenAiCompatible,
            config_file_paths: vec![
                "~/.config/cardnote-compiler/providers.json".to_string(),
                ".cardnote/providers.json".to_string(),
            ],
            description: "MiniMax M 系列大模型".to_string(),
            models: vec![
                ModelInfo {
                    id: "minimax-m2.7".to_string(),
                    name: "MiniMax M2.7".to_string(),
                    context_length: 204_800,
                    max_output_tokens: 131_072,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "MiniMax 2.7 — 204800上下文".to_string(),
                },
            ],
        });

        // 字节跳动豆包
        self.register(Provider {
            id: "bytedance".to_string(),
            name: "字节跳动".to_string(),
            aliases: vec!["doubao".to_string(), "volcengine".to_string()],
            default_base_url: "https://ark.cn-beijing.volces.com/api/v3".to_string(),
            api_key_env_var: "ARK_API_KEY".to_string(),
            api_key_pattern: "".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::OpenAiCompatible,
            config_file_paths: vec![
                "~/.config/cardnote-compiler/providers.json".to_string(),
                ".cardnote/providers.json".to_string(),
            ],
            description: "字节跳动豆包大模型".to_string(),
            models: vec![
                ModelInfo {
                    id: "doubao-pro-128k".to_string(),
                    name: "豆包 Pro 128K".to_string(),
                    context_length: 128_000,
                    max_output_tokens: 4096,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "专业版长上下文".to_string(),
                },
                ModelInfo {
                    id: "doubao-lite-128k".to_string(),
                    name: "豆包 Lite 128K".to_string(),
                    context_length: 128_000,
                    max_output_tokens: 4096,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "轻量版".to_string(),
                },
            ],
        });

        // Mistral
        self.register(Provider {
            id: "mistral".to_string(),
            name: "Mistral AI".to_string(),
            aliases: vec![],
            default_base_url: "https://api.mistral.ai/v1".to_string(),
            api_key_env_var: "MISTRAL_API_KEY".to_string(),
            api_key_pattern: "".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::OpenAiCompatible,
            config_file_paths: vec![
                "~/.config/cardnote-compiler/providers.json".to_string(),
                ".cardnote/providers.json".to_string(),
            ],
            description: "Mistral 欧洲大模型".to_string(),
            models: vec![
                ModelInfo {
                    id: "mistral-large".to_string(),
                    name: "Mistral Large".to_string(),
                    context_length: 32_000,
                    max_output_tokens: 4096,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "旗舰模型".to_string(),
                },
                ModelInfo {
                    id: "mistral-medium".to_string(),
                    name: "Mistral Medium".to_string(),
                    context_length: 32_000,
                    max_output_tokens: 4096,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "平衡性能".to_string(),
                },
            ],
        });

        // Cohere
        self.register(Provider {
            id: "cohere".to_string(),
            name: "Cohere".to_string(),
            aliases: vec![],
            default_base_url: "https://api.cohere.ai/v1".to_string(),
            api_key_env_var: "COHERE_API_KEY".to_string(),
            api_key_pattern: "".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::Cohere,
            config_file_paths: vec![
                "~/.config/cardnote-compiler/providers.json".to_string(),
                ".cardnote/providers.json".to_string(),
            ],
            description: "Cohere Command 系列".to_string(),
            models: vec![ModelInfo {
                id: "command-r-plus".to_string(),
                name: "Command R+".to_string(),
                context_length: 128_000,
                max_output_tokens: 4096,
                supports_json_mode: false,
                supports_vision: false,
                description: "长上下文RAG专用".to_string(),
            }],
        });

        // ZSCC 聚合平台
        self.register(Provider {
            id: "zscc".to_string(),
            name: "ZSCC".to_string(),
            aliases: vec!["zscc".to_string()],
            default_base_url: "https://api.zscc.in/v1".to_string(),
            api_key_env_var: "ZSCC_API_KEY".to_string(),
            api_key_pattern: "sk-".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::OpenAiCompatible,
            config_file_paths: vec![
                "~/.config/cardnote-compiler/providers.json".to_string(),
                ".cardnote/providers.json".to_string(),
            ],
            description: "ZSCC 聚合模型平台 (Claude/DeepSeek/GLM/GPT/Kimi/MiniMax)".to_string(),
            models: vec![
                ModelInfo {
                    id: "deepseek-v4-pro-cc".to_string(),
                    name: "DeepSeek V4 Pro".to_string(),
                    context_length: 1_000_000,
                    max_output_tokens: 8192,
                    supports_json_mode: false,
                    supports_vision: false,
                    description: "DeepSeek V4 Pro via ZSCC (1M上下文)".to_string(),
                },
                ModelInfo {
                    id: "deepseek-v4-flash-cc".to_string(),
                    name: "DeepSeek V4 Flash".to_string(),
                    context_length: 1_000_000,
                    max_output_tokens: 8192,
                    supports_json_mode: false,
                    supports_vision: false,
                    description: "DeepSeek V4 Flash via ZSCC (1M上下文)".to_string(),
                },
                ModelInfo {
                    id: "kimi-k2.6-cc".to_string(),
                    name: "Kimi K2.6".to_string(),
                    context_length: 256_000,
                    max_output_tokens: 96_000,
                    supports_json_mode: true,
                    supports_vision: true,
                    description: "Kimi K2.6 via ZSCC".to_string(),
                },
                ModelInfo {
                    id: "glm-5.1-cc".to_string(),
                    name: "GLM-5.1".to_string(),
                    context_length: 202_752,
                    max_output_tokens: 128_000,
                    supports_json_mode: true,
                    supports_vision: true,
                    description: "GLM-5.1 via ZSCC".to_string(),
                },
                ModelInfo {
                    id: "minimax-m2.7-cc".to_string(),
                    name: "MiniMax M2.7".to_string(),
                    context_length: 204_800,
                    max_output_tokens: 131_072,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "MiniMax M2.7 via ZSCC".to_string(),
                },
            ],
        });

        // NVIDIA NIM
        self.register(Provider {
            id: "nvidia".to_string(),
            name: "NVIDIA".to_string(),
            aliases: vec!["nvidia-nim".to_string()],
            default_base_url: "https://integrate.api.nvidia.com/v1".to_string(),
            api_key_env_var: "NVIDIA_API_KEY".to_string(),
            api_key_pattern: "nvapi-".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::OpenAiCompatible,
            config_file_paths: vec![
                "~/.config/cardnote-compiler/providers.json".to_string(),
                ".cardnote/providers.json".to_string(),
            ],
            description: "NVIDIA NIM 推理平台".to_string(),
            models: vec![
                ModelInfo {
                    id: "deepseek-ai/deepseek-v4-pro".to_string(),
                    name: "DeepSeek V4 Pro (NVIDIA)".to_string(),
                    context_length: 128_000,
                    max_output_tokens: 8192,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "DeepSeek V4 Pro via NVIDIA NIM".to_string(),
                },
                ModelInfo {
                    id: "deepseek-ai/deepseek-v4-flash".to_string(),
                    name: "DeepSeek V4 Flash (NVIDIA)".to_string(),
                    context_length: 128_000,
                    max_output_tokens: 8192,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "DeepSeek V4 Flash via NVIDIA NIM".to_string(),
                },
                ModelInfo {
                    id: "01-ai/yi-large".to_string(),
                    name: "Yi Large".to_string(),
                    context_length: 32_000,
                    max_output_tokens: 4096,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "Yi Large via NVIDIA NIM".to_string(),
                },
            ],
        });

        // Linux.do Hub
        self.register(Provider {
            id: "linuxdo".to_string(),
            name: "Linux.do Hub".to_string(),
            aliases: vec!["hub".to_string(), "linux-do".to_string()],
            default_base_url: "https://hub.linux.do/v1".to_string(),
            api_key_env_var: "LINUXDO_API_KEY".to_string(),
            api_key_pattern: "ah-".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::OpenAiCompatible,
            config_file_paths: vec![
                "~/.config/cardnote-compiler/providers.json".to_string(),
                ".cardnote/providers.json".to_string(),
            ],
            description: "Linux.do Hub 聚合平台".to_string(),
            models: vec![
                ModelInfo {
                    id: "deepseek-v4-pro".to_string(),
                    name: "DeepSeek V4 Pro".to_string(),
                    context_length: 128_000,
                    max_output_tokens: 8192,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "DeepSeek V4 Pro via Linux.do".to_string(),
                },
                ModelInfo {
                    id: "deepseek-v4-flash".to_string(),
                    name: "DeepSeek V4 Flash".to_string(),
                    context_length: 128_000,
                    max_output_tokens: 8192,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "DeepSeek V4 Flash via Linux.do".to_string(),
                },
            ],
        });

        // Hugging Face Router
        self.register(Provider {
            id: "huggingface".to_string(),
            name: "Hugging Face".to_string(),
            aliases: vec!["hf".to_string(), "huggingface-router".to_string()],
            default_base_url: "https://router.huggingface.co/v1".to_string(),
            api_key_env_var: "HF_API_KEY".to_string(),
            api_key_pattern: "hf_".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::OpenAiCompatible,
            config_file_paths: vec![
                "~/.config/cardnote-compiler/providers.json".to_string(),
                ".cardnote/providers.json".to_string(),
            ],
            description: "Hugging Face Router 推理平台".to_string(),
            models: vec![
                ModelInfo {
                    id: "zai-org/GLM-5.1".to_string(),
                    name: "GLM-5.1".to_string(),
                    context_length: 128_000,
                    max_output_tokens: 8192,
                    supports_json_mode: true,
                    supports_vision: true,
                    description: "智谱 GLM-5.1 via Hugging Face".to_string(),
                },
                ModelInfo {
                    id: "moonshotai/Kimi-K2.6".to_string(),
                    name: "Kimi K2.6".to_string(),
                    context_length: 256_000,
                    max_output_tokens: 8192,
                    supports_json_mode: true,
                    supports_vision: true,
                    description: "Kimi K2.6 via Hugging Face".to_string(),
                },
                ModelInfo {
                    id: "MiniMaxAI/MiniMax-M2.7".to_string(),
                    name: "MiniMax M2.7".to_string(),
                    context_length: 128_000,
                    max_output_tokens: 8192,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "MiniMax M2.7 via Hugging Face".to_string(),
                },
            ],
        });

        // LPGPT 聚合平台
        self.register(Provider {
            id: "lpgpt".to_string(),
            name: "LPGPT".to_string(),
            aliases: vec!["lp".to_string()],
            default_base_url: "https://lpgpt.us".to_string(),
            api_key_env_var: "LPGPT_API_KEY".to_string(),
            api_key_pattern: "sk-".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::OpenAiCompatible,
            config_file_paths: vec![
                "~/.config/cardnote-compiler/providers.json".to_string(),
                ".cardnote/providers.json".to_string(),
            ],
            description: "LPGPT 聚合模型平台".to_string(),
            models: vec![ModelInfo {
                id: "gpt-5.5".to_string(),
                name: "GPT 5.5".to_string(),
                context_length: 1_050_000,
                max_output_tokens: 128_000,
                supports_json_mode: true,
                supports_vision: true,
                description: "OpenAI GPT-5.5 via LPGPT".to_string(),
            }],
        });

        // Elysiver 聚合平台
        self.register(Provider {
            id: "elysiver".to_string(),
            name: "Elysiver".to_string(),
            aliases: vec!["ely".to_string()],
            default_base_url: "https://elysiver.h-e.top".to_string(),
            api_key_env_var: "ELYSIVER_API_KEY".to_string(),
            api_key_pattern: "sk-".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::OpenAiCompatible,
            config_file_paths: vec![
                "~/.config/cardnote-compiler/providers.json".to_string(),
                ".cardnote/providers.json".to_string(),
            ],
            description: "Elysiver 聚合模型平台".to_string(),
            models: vec![
                ModelInfo {
                    id: "glm-5.1-2cc".to_string(),
                    name: "GLM-5.1-2CC".to_string(),
                    context_length: 128_000,
                    max_output_tokens: 8192,
                    supports_json_mode: true,
                    supports_vision: true,
                    description: "智谱 GLM-5.1 via Elysiver".to_string(),
                },
                ModelInfo {
                    id: "deepseek-v4-pro-2cc".to_string(),
                    name: "DeepSeek V4 Pro-2CC".to_string(),
                    context_length: 128_000,
                    max_output_tokens: 8192,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "DeepSeek V4 Pro via Elysiver".to_string(),
                },
                ModelInfo {
                    id: "qwen3.6-plus-thinking".to_string(),
                    name: "Qwen 3.6 Plus Thinking".to_string(),
                    context_length: 1_000_000,
                    max_output_tokens: 65_536,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "通义千问 3.6 Plus Thinking via Elysiver".to_string(),
                },
            ],
        });

        // AI1216 聚合平台
        self.register(Provider {
            id: "ai1216".to_string(),
            name: "AI1216".to_string(),
            aliases: vec!["1216".to_string(), "ai-1216".to_string()],
            default_base_url: "https://ai.121628.xyz/v1".to_string(),
            api_key_env_var: "AI1216_API_KEY".to_string(),
            api_key_pattern: "sk-".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::OpenAiCompatible,
            config_file_paths: vec![
                "~/.config/cardnote-compiler/providers.json".to_string(),
                ".cardnote/providers.json".to_string(),
            ],
            description: "AI1216 聚合模型平台 (GLM/Kimi/MiniMax)".to_string(),
            models: vec![
                ModelInfo {
                    id: "glm-5.1".to_string(),
                    name: "GLM-5.1".to_string(),
                    context_length: 128_000,
                    max_output_tokens: 8192,
                    supports_json_mode: true,
                    supports_vision: true,
                    description: "智谱 GLM-5.1 via AI1216".to_string(),
                },
                ModelInfo {
                    id: "kimi-k2.6".to_string(),
                    name: "Kimi K2.6".to_string(),
                    context_length: 256_000,
                    max_output_tokens: 8192,
                    supports_json_mode: true,
                    supports_vision: true,
                    description: "Moonshot Kimi K2.6 via AI1216".to_string(),
                },
                ModelInfo {
                    id: "minimax-m2.7".to_string(),
                    name: "MiniMax M2.7".to_string(),
                    context_length: 128_000,
                    max_output_tokens: 8192,
                    supports_json_mode: true,
                    supports_vision: false,
                    description: "MiniMax M2.7 via AI1216".to_string(),
                },
            ],
        });

        // DGBMC 聚合平台
        self.register(Provider {
            id: "dgbmc".to_string(),
            name: "DGBMC".to_string(),
            aliases: vec!["dgbmc".to_string()],
            default_base_url: "https://freeapi.dgbmc.top/v1".to_string(),
            api_key_env_var: "DGBMC_API_KEY".to_string(),
            api_key_pattern: "sk-".to_string(),
            requires_api_key: true,
            protocol: ProviderProtocol::OpenAiCompatible,
            config_file_paths: vec![
                "~/.config/cardnote-compiler/providers.json".to_string(),
                ".cardnote/providers.json".to_string(),
            ],
            description: "DGBMC 聚合模型平台 (Gemini)".to_string(),
            models: vec![ModelInfo {
                id: "gemini-3.1-pro".to_string(),
                name: "Gemini 3.1 Pro".to_string(),
                context_length: 1_000_000,
                max_output_tokens: 8192,
                supports_json_mode: true,
                supports_vision: true,
                description: "Google Gemini 3.1 Pro via DGBMC".to_string(),
            }],
        });
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 扫描用户环境中的 AI 配置
pub fn scan_credentials() -> HashMap<String, ProviderCredential> {
    let mut found = HashMap::new();
    let registry = ProviderRegistry::new();

    // 0. 优先扫描项目级 .cardnote/providers.json
    let project_config_path = PathBuf::from(".cardnote/providers.json");
    let project_config_content = if project_config_path.exists() {
        std::fs::read_to_string(&project_config_path).ok()
    } else {
        None
    };

    for provider in registry.list_all() {
        // 1. 检查环境变量
        if let Ok(key) = std::env::var(&provider.api_key_env_var)
            && !key.is_empty()
            && provider.validate_key_format(&key)
        {
            found.insert(
                provider.id.clone(),
                ProviderCredential {
                    provider_id: provider.id.clone(),
                    api_key: key,
                    base_url: None,
                    default_model: provider.default_model().map(|m| m.id.clone()),
                    source: CredentialSource::EnvVar,
                },
            );
            continue;
        }

        // 2. 优先检查项目级 .cardnote/providers.json
        if let Some(ref content) = project_config_content {
            if let Some(cred) = extract_credential_from_file(provider, content, ".cardnote/providers.json") {
                found.insert(provider.id.clone(), cred);
                continue;
            }
        }

        // 3. 检查 provider 默认配置文件路径
        for path in &provider.config_file_paths {
            let expanded = shellexpand::tilde(path).to_string();
            if let Ok(content) = std::fs::read_to_string(&expanded)
                && let Some(cred) = extract_credential_from_file(provider, &content, &expanded)
            {
                found.insert(provider.id.clone(), cred);
                break;
            }
        }
    }

    // 特殊处理：扫描 Claude Code 配置中的代理设置
    if let Ok(content) =
        std::fs::read_to_string(shellexpand::tilde("~/.claude/settings.json").as_ref())
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
    {
        // 检查是否有 env 中的 ANTHROPIC_BASE_URL 和 ANTHROPIC_AUTH_TOKEN
        if let Some(env) = json.get("env")
            && let (Some(base_url), Some(auth_token)) = (
                env.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()),
                env.get("ANTHROPIC_AUTH_TOKEN").and_then(|v| v.as_str()),
            )
            && !auth_token.is_empty()
        {
            // 这是通过代理的 Claude 配置，使用代理的 base_url
            found.insert(
                "anthropic".to_string(),
                ProviderCredential {
                    provider_id: "anthropic".to_string(),
                    api_key: auth_token.to_string(),
                    base_url: Some(base_url.to_string()),
                    default_model: Some("claude-sonnet-4-6".to_string()),
                    source: CredentialSource::ConfigFile("~/.claude/settings.json".to_string()),
                },
            );
        }
    }

    // 特殊处理：扫描 PDFMathTranslate 配置中的 DeepSeek
    let pdfmath_path = shellexpand::tilde("~/.config/PDFMathTranslate/config.json").to_string();
    if let Ok(content) = std::fs::read_to_string(&pdfmath_path)
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
        && let Some(translators) = json.get("translators").and_then(|v| v.as_array())
    {
        for translator in translators {
            if translator.get("name").and_then(|v| v.as_str()) == Some("deepseek")
                && let Some(envs) = translator.get("envs")
                && let Some(key) = envs.get("DEEPSEEK_API_KEY").and_then(|v| v.as_str())
                && !key.is_empty()
                && !found.contains_key("deepseek")
            {
                found.insert(
                    "deepseek".to_string(),
                    ProviderCredential {
                        provider_id: "deepseek".to_string(),
                        api_key: key.to_string(),
                        base_url: None,
                        default_model: envs
                            .get("DEEPSEEK_MODEL")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        source: CredentialSource::ConfigFile(
                            "~/.config/PDFMathTranslate/config.json".to_string(),
                        ),
                    },
                );
            }
        }
    }

    // 特殊处理：扫描 Codex CLI 配置
    // Codex 使用 ~/.config/alma/ 或其他位置存储配置
    // 通过环境变量或命令行获取

    found
}

/// 从配置文件中提取凭据
fn extract_credential_from_file(
    provider: &Provider,
    content: &str,
    path: &str,
) -> Option<ProviderCredential> {
    // 尝试 JSON 解析
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(content) {
        // 1. 先尝试嵌套格式: {"zscc": {"api_key": "...", "base_url": "..."}}
        if let Some(provider_block) = json.get(&provider.id).and_then(|v| v.as_object()) {
            for key_field in ["api_key", "apiKey", "key", "token"] {
                if let Some(key) = provider_block.get(key_field).and_then(|v| v.as_str())
                    && !key.is_empty()
                    && provider.validate_key_format(key)
                {
                    let base_url = provider_block
                        .get("base_url")
                        .or_else(|| provider_block.get("baseUrl"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let model = provider_block
                        .get("model")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    return Some(ProviderCredential {
                        provider_id: provider.id.clone(),
                        api_key: key.to_string(),
                        base_url,
                        default_model: model
                            .or_else(|| provider.default_model().map(|m| m.id.clone())),
                        source: CredentialSource::ConfigFile(path.to_string()),
                    });
                }
            }
        }

        // 2. 回退到扁平格式: {"api_key": "..."}
        for key_field in ["api_key", "apiKey", "key", "token"] {
            if let Some(key) = json.get(key_field).and_then(|v| v.as_str())
                && !key.is_empty()
                && provider.validate_key_format(key)
            {
                let base_url = json
                    .get("base_url")
                    .or_else(|| json.get("baseUrl"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let model = json
                    .get("model")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                return Some(ProviderCredential {
                    provider_id: provider.id.clone(),
                    api_key: key.to_string(),
                    base_url,
                    default_model: model.or_else(|| provider.default_model().map(|m| m.id.clone())),
                    source: CredentialSource::ConfigFile(path.to_string()),
                });
            }
        }
    }

    None
}

/// 生成配置状态报告
pub fn generate_status_report(found: &HashMap<String, ProviderCredential>) -> String {
    let registry = ProviderRegistry::new();
    let mut lines = vec![
        "╔════════════════════════════════════════════════════════════╗".to_string(),
        "║              AI 提供商配置状态                            ║".to_string(),
        "╠════════════════════════════════════════════════════════════╣".to_string(),
    ];

    for provider in registry.list_all() {
        let status = if let Some(cred) = found.get(&provider.id) {
            let model_info = cred
                .default_model
                .as_ref()
                .and_then(|m| provider.find_model(m))
                .map(|m| format!(" [{}]", m.name))
                .unwrap_or_default();
            let source_icon = match cred.source {
                CredentialSource::EnvVar => "🌐",
                CredentialSource::ConfigFile(_) => "📄",
                CredentialSource::UserInput => "✏️",
                CredentialSource::AutoDetected => "🔍",
            };
            format!(
                "✅ 已配置 {}{} {}",
                source_icon,
                model_info,
                format_source(&cred.source)
            )
        } else {
            format!("❌ 未配置 — 设置环境变量 {}", provider.api_key_env_var)
        };

        lines.push(format!(
            "║ {:<12} {:<43}║",
            provider.name,
            status.chars().take(43).collect::<String>()
        ));
    }

    lines.push("╚════════════════════════════════════════════════════════════╝".to_string());
    lines.join("\n")
}

fn format_source(source: &CredentialSource) -> String {
    match source {
        CredentialSource::EnvVar => "(环境变量)".to_string(),
        CredentialSource::ConfigFile(path) => format!("({})", path),
        CredentialSource::UserInput => "(手动输入)".to_string(),
        CredentialSource::AutoDetected => "(自动检测)".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = ProviderRegistry::new();
        assert!(!registry.list_all().is_empty());
        assert!(registry.get("openai").is_some());
        assert!(registry.get("deepseek").is_some());
    }

    #[test]
    fn test_find_by_alias() {
        let registry = ProviderRegistry::new();
        assert!(registry.find_by_alias("claude").is_some());
        assert!(registry.find_by_alias("codex").is_some());
        assert!(registry.find_by_alias("kimi").is_some());
    }

    #[test]
    fn test_validate_key_format() {
        let registry = ProviderRegistry::new();
        let anthropic = registry.get("anthropic").unwrap();
        assert!(anthropic.validate_key_format("sk-ant-test123"));
        assert!(!anthropic.validate_key_format("invalid-key"));
    }

    #[test]
    fn test_generate_status_report() {
        let mut found = HashMap::new();
        found.insert(
            "deepseek".to_string(),
            ProviderCredential {
                provider_id: "deepseek".to_string(),
                api_key: "sk-test".to_string(),
                base_url: None,
                default_model: Some("deepseek-chat".to_string()),
                source: CredentialSource::EnvVar,
            },
        );
        let report = generate_status_report(&found);
        assert!(report.contains("DeepSeek"));
        assert!(report.contains("✅"));
    }
}
