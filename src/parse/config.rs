use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlamaParseConfig {
    pub api_key: Option<String>,
    pub num_ongoing_requests: usize,
    pub base_url: Option<String>,
    pub parse_kwargs: HashMap<String, String>,
    pub check_interval: u64,
    pub max_timeout: u64,
    pub max_retries: usize,
    pub retry_delay_ms: u64,
    pub backoff_multiplier: f64,
}

impl Default for LlamaParseConfig {
    fn default() -> Self {
        Self {
            api_key: std::env::var("LLAMA_CLOUD_API_KEY").ok(),
            num_ongoing_requests: 10,
            base_url: Some("https://api.cloud.llamaindex.ai".to_string()),
            parse_kwargs: HashMap::from([
                (
                    "parse_mode".to_string(),
                    "parse_page_with_agent".to_string(),
                ),
                ("model".to_string(), "openai-gpt-4-1-mini".to_string()),
                ("high_res_ocr".to_string(), "true".to_string()),
                ("adaptive_long_table".to_string(), "true".to_string()),
                ("outlined_table_extraction".to_string(), "true".to_string()),
                ("output_tables_as_HTML".to_string(), "true".to_string()),
            ]),
            check_interval: 5,
            max_timeout: 3600,
            max_retries: 10,
            retry_delay_ms: 1000,
            backoff_multiplier: 2.0,
        }
    }
}

impl LlamaParseConfig {
    pub fn from_config_file(path: &str) -> anyhow::Result<Self> {
        if !Path::new(path).exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(path)?;
        let config: LlamaParseConfig = serde_json::from_str(&contents)?;
        Ok(config)
    }
} 