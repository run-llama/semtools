use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[cfg(feature = "parse")]
use crate::parse::LlamaParseConfig;

/// Unified configuration for all semtools CLI tools
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SemtoolsConfig {
    /// Configuration for the parse CLI tool
    #[cfg(feature = "parse")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse: Option<LlamaParseConfig>,

    /// Configuration for the ask CLI tool
    #[cfg(feature = "ask")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<AskConfig>,
}

/// Configuration for the ask CLI tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskConfig {
    /// OpenAI API key (can also be set via OPENAI_API_KEY env var)
    pub api_key: Option<String>,

    /// OpenAI base URL (defaults to OpenAI's API)
    pub base_url: Option<String>,

    /// Default model to use for the agent (e.g., "gpt-4o-mini", "gpt-4")
    pub model: Option<String>,

    /// Maximum number of agent loop iterations
    pub max_iterations: Option<usize>,
}

impl Default for AskConfig {
    fn default() -> Self {
        Self {
            api_key: std::env::var("OPENAI_API_KEY").ok(),
            base_url: None,
            model: Some("gpt-4o-mini".to_string()),
            max_iterations: Some(10),
        }
    }
}

impl SemtoolsConfig {
    /// Load configuration from a file path
    /// If the file doesn't exist, returns default configuration
    pub fn from_config_file(path: &str) -> anyhow::Result<Self> {
        if !Path::new(path).exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(path)?;
        let config: SemtoolsConfig = serde_json::from_str(&contents)?;
        Ok(config)
    }

    /// Get the default config file path (~/.semtools_config.json)
    pub fn default_config_path() -> String {
        dirs::home_dir()
            .unwrap()
            .join(".semtools_config.json")
            .to_string_lossy()
            .to_string()
    }

    /// Load configuration from the default path
    pub fn load_default() -> anyhow::Result<Self> {
        Self::from_config_file(&Self::default_config_path())
    }
}
