use std::fs;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::parse::cache::CacheManager;
use crate::parse::client::ParseClient;
use crate::parse::config::LlamaParseConfig;
use crate::parse::error::JobError;

pub struct LlamaParseBackend {
    config: LlamaParseConfig,
    cache_manager: CacheManager,
    verbose: bool,
}

impl LlamaParseBackend {
    pub fn new(config: LlamaParseConfig, verbose: bool) -> anyhow::Result<Self> {
        let cache_dir = dirs::home_dir()
            .ok_or_else(|| anyhow::Error::msg("Could not find home directory"))?
            .join(".parse");

        fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            config,
            cache_manager: CacheManager::new(cache_dir),
            verbose,
        })
    }

    pub async fn parse(&self, files: Vec<String>) -> Result<Vec<String>, JobError> {
        let semaphore = Arc::new(Semaphore::new(self.config.num_ongoing_requests));

        let base_url = self
            .config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.cloud.llamaindex.ai".to_string());
        let api_key = self
            .config
            .api_key
            .clone()
            .unwrap_or_else(|| std::env::var("LLAMA_CLOUD_API_KEY").unwrap_or_default());

        let mut handles = Vec::new();
        let mut results = Vec::new();

        for file_path in files {
            // Skip if file doesn't need parsing
            if self.cache_manager.should_skip_file(&file_path) {
                if self.verbose {
                    eprintln!("Skipping readable file: {file_path}");
                }
                results.push(file_path);
                continue;
            }

            // Check cache first
            if let Ok(cached_path) = self.cache_manager.get_cached_result(&file_path).await {
                if self.verbose {
                    eprintln!("Using cached result for: {file_path}");
                }
                results.push(cached_path);
                continue;
            }

            let semaphore = Arc::clone(&semaphore);
            let base_url = base_url.clone();
            let api_key = api_key.clone();
            let config = self.config.clone();
            let cache_manager = CacheManager::new(self.cache_manager.cache_dir.clone());
            let client = ParseClient::new();
            let verbose = self.verbose;

            let handle = tokio::spawn(async move {
                let _permit = semaphore.acquire_owned().await.unwrap();

                Self::process_single_document(
                    client, file_path, base_url, api_key, config, cache_manager, verbose,
                )
                .await
            });

            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            let result = handle.await?;
            match result {
                Ok(path) => results.push(path),
                Err(e) => eprintln!("Error processing file: {e:?}"),
            }
        }

        Ok(results)
    }

    async fn process_single_document(
        client: ParseClient,
        file_path: String,
        base_url: String,
        api_key: String,
        config: LlamaParseConfig,
        cache_manager: CacheManager,
        verbose: bool,
    ) -> Result<String, JobError> {
        if verbose {
            eprintln!("Processing file: {file_path}");
        }

        // Create job with retry
        let job_id = client
            .create_parse_job_with_retry(&file_path, &base_url, &api_key, &config)
            .await?;

        // Poll for result with retry
        let markdown_content = client
            .poll_for_result_with_retry(&job_id, &base_url, &api_key, &config)
            .await?;

        // Write results to disk
        cache_manager
            .write_results_to_disk(&file_path, &markdown_content)
            .await
    }
} 