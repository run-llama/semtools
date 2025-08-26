use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Debug, Serialize, Deserialize)]
struct LlamaParseConfig {
    api_key: Option<String>,
    num_ongoing_requests: usize,
    base_url: Option<String>,
    parse_kwargs: HashMap<String, String>,
}

impl LlamaParseConfig {
    pub fn from_config_file(path: &str) -> anyhow::Result<Self, Box<dyn std::error::Error>> {
        // Read the file contents
        let contents = fs::read_to_string(path)?;

        // Parse JSON into the struct
        let config: LlamaParseConfig = serde_json::from_str(&contents)?;

        Ok(config)
    }
}

#[derive(Debug)]
pub enum JobError {
    HttpError(reqwest::Error),
    IoError(std::io::Error),
    TimeoutError,
    InvalidResponse,
    JoinError(tokio::task::JoinError),
}

impl From<reqwest::Error> for JobError {
    fn from(err: reqwest::Error) -> Self {
        JobError::HttpError(err)
    }
}

impl From<std::io::Error> for JobError {
    fn from(err: std::io::Error) -> Self {
        JobError::IoError(err)
    }
}

impl From<tokio::task::JoinError> for JobError {
    fn from(err: tokio::task::JoinError) -> Self {
        JobError::JoinError(err)
    }
}

struct LlamaParseBackend {
    config: LlamaParseConfig,
}

impl LlamaParseBackend {
    pub async fn parse(&self, files: Vec<String>) -> Result<Vec<String>, JobError> {
        let client = Arc::new(Client::new());
        let semaphore = Arc::new(Semaphore::new(self.config.num_ongoing_requests));

        // Clone values we need to move into the tasks
        let base_url = self
            .config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.cloud.llamaindex.ai".to_string());
        let api_key = self
            .config
            .api_key
            .clone()
            .unwrap_or_else(|| std::env::var("LLAMA_CLOUD_API_KEY").unwrap());
        let parse_kwargs = self.config.parse_kwargs.clone();

        let mut handles = Vec::new();

        for file_path in files {
            let client = Arc::clone(&client);
            let semaphore = Arc::clone(&semaphore);
            let base_url = base_url.clone();
            let api_key = api_key.clone();
            let parse_kwargs = parse_kwargs.clone();

            let handle = tokio::spawn(async move {
                let _permit = semaphore.acquire_owned().await.unwrap();

                Self::process_single_document(client, file_path, base_url, api_key, parse_kwargs)
                    .await
            });

            handles.push(handle);
        }

        // Wait for all tasks to complete
        let mut results = Vec::new();
        for handle in handles {
            let result = handle.await?;
            match result {
                Ok(content) => results.push(content),
                Err(e) => println!("Error processing file: {:?}", e),
            }
        }

        Ok(results)
    }

    async fn process_single_document(
        client: Arc<reqwest::Client>,
        file: String,
        base_url: String,
        api_key: String,
        parse_kwargs: HashMap<String, String>,
    ) -> Result<String, JobError> {
        println!("Processing file: {}", file);

        // Example of using the client for an actual request:
        // let response = client
        //     .post(&format!("{}/parse", base_url))
        //     .header("Authorization", format!("Bearer {}", api_key))
        //     .send()
        //     .await?;

        Ok(format!("Processed: {}", file))
    }

    async fn create_parse_job() {}

    async fn poll_for_result() {}

    async fn write_results_to_disk() {}
}
