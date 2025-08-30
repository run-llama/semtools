use async_trait::async_trait;
use reqwest;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::parse::backend_trait::ParseBackend;
use crate::parse::cache::CacheManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LMStudioConfig {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,
    #[serde(default = "default_chunk_overlap")]
    pub chunk_overlap: usize,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
    #[serde(default = "default_num_ongoing_requests")]
    pub num_ongoing_requests: usize,
}

fn default_base_url() -> String {
    "http://localhost:1234/v1".to_string()
}

fn default_model() -> String {
    "local-model".to_string()
}

fn default_temperature() -> f32 {
    0.3
}

fn default_max_tokens() -> u32 {
    4096
}

fn default_chunk_size() -> usize {
    3000
}

fn default_chunk_overlap() -> usize {
    200
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_delay_ms() -> u64 {
    1000
}

fn default_num_ongoing_requests() -> usize {
    5
}

impl Default for LMStudioConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            model: default_model(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            chunk_size: default_chunk_size(),
            chunk_overlap: default_chunk_overlap(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay_ms(),
            num_ongoing_requests: default_num_ongoing_requests(),
        }
    }
}

impl LMStudioConfig {
    /// Load configuration with intelligent fallback logic
    pub fn from_config_file(path: &str) -> anyhow::Result<Self> {
        // Try the specified path first
        if Path::new(path).exists() {
            let contents = fs::read_to_string(path).map_err(|e| {
                anyhow::Error::msg(format!("Failed to read config file {}: {}", path, e))
            })?;

            let config: LMStudioConfig = serde_json::from_str(&contents).map_err(|e| {
                anyhow::Error::msg(format!("Invalid JSON in config file {}: {}", path, e))
            })?;

            return Ok(config);
        }

        // If using default LMStudio config path, try some fallbacks
        if path.ends_with(".lmstudio_parse_config.json")
            && let Some(home) = dirs::home_dir()
        {
            // Try alternative names people might use
            let fallback_paths = [
                home.join(".lmstudio_config.json"),
                home.join(".lmstudio.json"),
                home.join(".parse_config.json"), // General fallback
            ];

            for fallback_path in &fallback_paths {
                if fallback_path.exists() {
                    eprintln!("ℹ️  Using fallback config: {}", fallback_path.display());
                    let contents = fs::read_to_string(fallback_path)?;

                    // Try to parse as LMStudio config first
                    if let Ok(config) = serde_json::from_str::<LMStudioConfig>(&contents) {
                        return Ok(config);
                    }

                    // If that fails, it might be a general config with some compatible fields
                    eprintln!(
                        "ℹ️  Config file format not fully compatible, using defaults with any compatible settings"
                    );
                }
            }
        }

        // Check for environment variables as fallback
        let mut config = Self::default();

        // Allow base_url override via environment
        if let Ok(base_url) = std::env::var("LMSTUDIO_BASE_URL") {
            config.base_url = base_url;
        }

        // Allow model override via environment
        if let Ok(model) = std::env::var("LMSTUDIO_MODEL") {
            config.model = model;
        }

        Ok(config)
    }

    /// Get the expected config file path
    pub fn default_config_path() -> Option<String> {
        dirs::home_dir().map(|home| {
            home.join(".lmstudio_parse_config.json")
                .to_string_lossy()
                .to_string()
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Choice {
    message: ChatMessage,
}

pub struct LMStudioBackend {
    config: LMStudioConfig,
    cache_manager: CacheManager,
    verbose: bool,
    client: reqwest::Client,
}

impl LMStudioBackend {
    pub fn new(config: LMStudioConfig, verbose: bool) -> anyhow::Result<Self> {
        let cache_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::Error::msg("Could not find home directory"))?
            .join(".parse")
            .join("lmstudio");

        fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            config,
            cache_manager: CacheManager::new(cache_dir),
            verbose,
            client: reqwest::Client::new(),
        })
    }

    async fn parse_document(&self, file_path: &str) -> anyhow::Result<String> {
        // Read the file content
        let content = fs::read_to_string(file_path)?;

        // Split into chunks if necessary
        let chunks = self.split_into_chunks(&content);
        let mut parsed_content = String::new();

        for (i, chunk) in chunks.iter().enumerate() {
            if self.verbose {
                eprintln!(
                    "Processing chunk {}/{} of {}",
                    i + 1,
                    chunks.len(),
                    file_path
                );
            }

            let mut retries = 0;
            loop {
                match self.send_parse_request(chunk, file_path).await {
                    Ok(response) => {
                        parsed_content.push_str(&response);
                        parsed_content.push('\n');
                        break;
                    }
                    Err(e) if retries < self.config.max_retries => {
                        if self.verbose {
                            eprintln!(
                                "Retry {}/{} for chunk {} of {}: {}",
                                retries + 1,
                                self.config.max_retries,
                                i + 1,
                                file_path,
                                e
                            );
                        }
                        tokio::time::sleep(tokio::time::Duration::from_millis(
                            self.config.retry_delay_ms * (retries as u64 + 1),
                        ))
                        .await;
                        retries += 1;
                    }
                    Err(e) => {
                        return Err(anyhow::Error::msg(format!(
                            "Failed to parse chunk {} of {}: {}",
                            i + 1,
                            file_path,
                            e
                        )));
                    }
                }
            }
        }

        Ok(parsed_content)
    }

    /// Sanitize filename to prevent injection attacks in prompts
    fn sanitize_filename(filename: &str) -> String {
        use std::path::Path;

        // Extract just the filename without path components
        let path = Path::new(filename);
        let clean_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("document");

        // Keep only safe characters
        clean_name
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(*c, '.' | '-' | '_' | ' '))
            .take(50) // Limit length
            .collect()
    }

    fn split_into_chunks(&self, content: &str) -> Vec<String> {
        let mut chunks = Vec::new();
        let chars: Vec<char> = content.chars().collect();
        let total_len = chars.len();

        if total_len <= self.config.chunk_size {
            chunks.push(content.to_string());
            return chunks;
        }

        let mut start = 0;
        while start < total_len {
            let end = (start + self.config.chunk_size).min(total_len);

            // Try to find a good breaking point (paragraph or sentence end)
            let mut actual_end = end;
            if end < total_len {
                // Look for paragraph break, but within reasonable bounds
                let search_start = start.max(end.saturating_sub(100));

                for i in (search_start..end).rev() {
                    if i < chars.len() && (chars[i] == '\n' || chars[i] == '.') {
                        actual_end = (i + 1).min(total_len);
                        break;
                    }
                }
            }

            let chunk: String = chars[start..actual_end].iter().collect();
            chunks.push(chunk);

            // Move to next chunk with overlap
            let next_start = actual_end.saturating_sub(self.config.chunk_overlap);
            // Ensure we make progress to avoid infinite loop
            start = start.max(next_start).max(start + 1);

            if start >= actual_end {
                start = actual_end;
            }
        }

        chunks
    }

    async fn send_parse_request(&self, content: &str, filename: &str) -> anyhow::Result<String> {
        // Sanitize filename for use in prompts to prevent injection
        let safe_filename = Self::sanitize_filename(filename);

        let system_prompt = format!(
            "You are a document parsing assistant. Parse the following content from '{}' \
             and convert it to clean, well-formatted markdown. Preserve the structure, \
             headings, lists, and important formatting. Do not add any commentary or \
             explanations, only return the parsed content.",
            safe_filename
        );

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt,
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: content.to_string(),
                },
            ],
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
        };

        let url = format!("{}/chat/completions", self.config.base_url);

        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::Error::msg(format!(
                "LMStudio API error: {}",
                error_text
            )));
        }

        let chat_response: ChatResponse = response.json().await?;

        if let Some(choice) = chat_response.choices.first() {
            Ok(choice.message.content.clone())
        } else {
            Err(anyhow::Error::msg("No response from LMStudio"))
        }
    }

    async fn process_single_file(&self, file_path: String) -> anyhow::Result<String> {
        // Skip if file doesn't need parsing (already markdown/text)
        if self.cache_manager.should_skip_file(&file_path) {
            if self.verbose {
                eprintln!("Skipping readable file: {}", file_path);
            }
            return Ok(file_path);
        }

        // Check cache first
        if let Ok(cached_path) = self.cache_manager.get_cached_result(&file_path).await {
            if self.verbose {
                eprintln!("Using cached result for: {}", file_path);
            }
            return Ok(cached_path);
        }

        if self.verbose {
            eprintln!("Processing file with LMStudio: {}", file_path);
        }

        // Parse the document
        let parsed_content = self.parse_document(&file_path).await?;

        // Write results to disk and cache
        self.cache_manager
            .write_results_to_disk(&file_path, &parsed_content)
            .await
            .map_err(|e| anyhow::Error::msg(format!("Failed to write results to disk: {}", e)))
    }
}

#[async_trait]
impl ParseBackend for LMStudioBackend {
    async fn parse(&self, files: Vec<String>) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
        let semaphore = Arc::new(Semaphore::new(self.config.num_ongoing_requests));
        let mut handles = Vec::new();

        for file_path in files {
            let semaphore = Arc::clone(&semaphore);
            let backend = LMStudioBackend {
                config: self.config.clone(),
                cache_manager: CacheManager::new(self.cache_manager.cache_dir.clone()),
                verbose: self.verbose,
                client: reqwest::Client::new(),
            };

            let handle = tokio::spawn(async move {
                let _permit = semaphore.acquire_owned().await.unwrap();
                backend.process_single_file(file_path).await
            });

            handles.push(handle);
        }

        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(Ok(path)) => results.push(path),
                Ok(Err(e)) => {
                    eprintln!("Error processing file: {:?}", e);
                    return Err(e.into());
                }
                Err(e) => {
                    eprintln!("Task error: {:?}", e);
                    return Err(e.into());
                }
            }
        }

        Ok(results)
    }

    fn name(&self) -> &str {
        "lmstudio"
    }

    fn verbose(&self) -> bool {
        self.verbose
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_lmstudio_config_default() {
        let config = LMStudioConfig::default();
        assert_eq!(config.base_url, "http://localhost:1234/v1");
        assert_eq!(config.model, "local-model");
        assert_eq!(config.temperature, 0.3);
        assert_eq!(config.max_tokens, 4096);
        assert_eq!(config.chunk_size, 3000);
        assert_eq!(config.chunk_overlap, 200);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 1000);
        assert_eq!(config.num_ongoing_requests, 5);
    }

    #[test]
    fn test_lmstudio_config_from_file() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let file_path = dir.path().join("config.json");
        let mut tmp_file = File::create(&file_path)?;
        writeln!(
            tmp_file,
            r#"{{
                "base_url": "http://localhost:8080/v1",
                "model": "custom-model",
                "temperature": 0.5,
                "max_tokens": 2048
            }}"#
        )?;

        let config = LMStudioConfig::from_config_file(&file_path.to_string_lossy())?;
        assert_eq!(config.base_url, "http://localhost:8080/v1");
        assert_eq!(config.model, "custom-model");
        assert_eq!(config.temperature, 0.5);
        assert_eq!(config.max_tokens, 2048);
        // Other values should use defaults
        assert_eq!(config.chunk_size, 3000);
        Ok(())
    }

    #[test]
    fn test_lmstudio_config_from_nonexistent_file() -> anyhow::Result<()> {
        let config = LMStudioConfig::from_config_file("/nonexistent/path.json")?;
        // Should return default config
        assert_eq!(config.base_url, "http://localhost:1234/v1");
        Ok(())
    }

    #[test]
    fn test_lmstudio_backend_creation() -> anyhow::Result<()> {
        let config = LMStudioConfig::default();
        let backend = LMStudioBackend::new(config, false)?;
        assert_eq!(backend.name(), "lmstudio");
        assert!(!backend.verbose());
        Ok(())
    }

    #[test]
    fn test_chunk_splitting() -> anyhow::Result<()> {
        let config = LMStudioConfig {
            chunk_size: 20,
            chunk_overlap: 5,
            ..Default::default()
        };
        let backend = LMStudioBackend::new(config, false)?;

        let content =
            "This is a test document with multiple sentences. It should be split into chunks.";
        let chunks = backend.split_into_chunks(content);

        assert!(
            chunks.len() > 1,
            "Content should be split into multiple chunks"
        );
        assert!(
            chunks[0].len() <= 20 + 5,
            "First chunk should respect size limits"
        ); // Allow for overlap
        Ok(())
    }

    #[test]
    fn test_small_content_no_splitting() -> anyhow::Result<()> {
        let config = LMStudioConfig::default();
        let backend = LMStudioBackend::new(config, false)?;

        let content = "Short content";
        let chunks = backend.split_into_chunks(content);

        assert_eq!(chunks.len(), 1, "Short content should not be split");
        assert_eq!(chunks[0], content, "Chunk should match original content");
        Ok(())
    }

    #[test]
    fn test_filename_sanitization() {
        // Test dangerous filenames
        assert_eq!(
            LMStudioBackend::sanitize_filename("../../../etc/passwd"),
            "passwd"
        );
        assert_eq!(LMStudioBackend::sanitize_filename("/etc/shadow"), "shadow");
        assert_eq!(
            LMStudioBackend::sanitize_filename("file'; rm -rf /"),
            "file rm -rf "
        );
        assert_eq!(
            LMStudioBackend::sanitize_filename("normal-file.pdf"),
            "normal-file.pdf"
        );

        // Test length limiting
        let long_name = "a".repeat(100);
        let sanitized = LMStudioBackend::sanitize_filename(&long_name);
        assert!(sanitized.len() <= 50, "Filename should be truncated");

        // Test empty/invalid names
        assert_eq!(LMStudioBackend::sanitize_filename(""), "document");
        assert_eq!(LMStudioBackend::sanitize_filename("../"), "document");
    }
}
