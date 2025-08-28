use reqwest::{Client, multipart};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Semaphore;
use tokio::time::sleep;

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

#[derive(Debug, Serialize, Deserialize)]
struct JobResponse {
    id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct JobStatus {
    status: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct JobResult {
    markdown: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileMetadata {
    modified_time: u64,
    size: u64,
    parsed_path: String,
}

#[derive(Debug)]
pub enum JobError {
    HttpError(reqwest::Error),
    IoError(std::io::Error),
    TimeoutError,
    InvalidResponse(String),
    JoinError(tokio::task::JoinError),
    SerializationError(serde_json::Error),
    RetryExhausted(String),
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

impl From<serde_json::Error> for JobError {
    fn from(err: serde_json::Error) -> Self {
        JobError::SerializationError(err)
    }
}

impl std::fmt::Display for JobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobError::HttpError(err) => write!(f, "HTTP error: {err}"),
            JobError::IoError(err) => write!(f, "IO error: {err}"),
            JobError::TimeoutError => write!(f, "Operation timed out"),
            JobError::InvalidResponse(msg) => write!(f, "Invalid response: {msg}"),
            JobError::JoinError(err) => write!(f, "Task join error: {err}"),
            JobError::SerializationError(err) => write!(f, "Serialization error: {err}"),
            JobError::RetryExhausted(msg) => write!(f, "Retry attempts exhausted: {msg}"),
        }
    }
}

impl std::error::Error for JobError {}

pub struct LlamaParseBackend {
    config: LlamaParseConfig,
    cache_dir: PathBuf,
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
            cache_dir,
            verbose,
        })
    }

    pub async fn parse(&self, files: Vec<String>) -> Result<Vec<String>, JobError> {
        let client = Arc::new(Client::new());
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
            if self.should_skip_file(&file_path) {
                if self.verbose {
                    eprintln!("Skipping readable file: {file_path}");
                }
                results.push(file_path);
                continue;
            }

            // Check cache first
            if let Ok(cached_path) = self.get_cached_result(&file_path).await {
                if self.verbose {
                    eprintln!("Using cached result for: {file_path}");
                }
                results.push(cached_path);
                continue;
            }

            let client = Arc::clone(&client);
            let semaphore = Arc::clone(&semaphore);
            let base_url = base_url.clone();
            let api_key = api_key.clone();
            let config = self.config.clone();
            let cache_dir = self.cache_dir.clone();
            let verbose = self.verbose;

            let handle = tokio::spawn(async move {
                let _permit = semaphore.acquire_owned().await.unwrap();

                Self::process_single_document(
                    client, file_path, base_url, api_key, config, cache_dir, verbose,
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

    fn should_skip_file(&self, file_path: &str) -> bool {
        let path = Path::new(file_path);

        // Skip if file doesn't exist
        if !path.exists() {
            return true;
        }

        // Skip readable text files
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            matches!(
                extension.to_lowercase().as_str(),
                "txt"
                    | "md"
                    | "rst"
                    | "org"
                    | "csv"
                    | "json"
                    | "xml"
                    | "yaml"
                    | "yml"
                    | "py"
                    | "js"
                    | "ts"
                    | "rs"
            )
        } else {
            false
        }
    }

    async fn get_cached_result(&self, file_path: &str) -> Result<String, JobError> {
        let metadata = self.get_file_metadata(file_path)?;
        let metadata_path = self.get_metadata_path(file_path);

        if !metadata_path.exists() {
            return Err(JobError::InvalidResponse("No cached metadata".to_string()));
        }

        let cached_metadata: FileMetadata =
            serde_json::from_str(&fs::read_to_string(metadata_path)?)?;

        // Check if file has changed
        if cached_metadata.modified_time == metadata.modified_time
            && cached_metadata.size == metadata.size
            && Path::new(&cached_metadata.parsed_path).exists()
        {
            Ok(cached_metadata.parsed_path)
        } else {
            Err(JobError::InvalidResponse("Cache invalid".to_string()))
        }
    }

    fn get_file_metadata(&self, file_path: &str) -> Result<FileMetadata, JobError> {
        let path = Path::new(file_path);
        let metadata = fs::metadata(path)?;

        let modified_time = metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Ok(FileMetadata {
            modified_time,
            size: metadata.len(),
            parsed_path: String::new(), // Will be set later
        })
    }

    fn get_metadata_path(&self, file_path: &str) -> PathBuf {
        let path = Path::new(file_path);
        let filename = path.file_name().unwrap().to_str().unwrap();
        self.cache_dir.join(format!("{filename}.metadata.json"))
    }

    async fn process_single_document(
        client: Arc<reqwest::Client>,
        file_path: String,
        base_url: String,
        api_key: String,
        config: LlamaParseConfig,
        cache_dir: PathBuf,
        verbose: bool,
    ) -> Result<String, JobError> {
        if verbose {
            eprintln!("Processing file: {file_path}");
        }

        // Create job with retry
        let job_id =
            Self::create_parse_job_with_retry(&client, &file_path, &base_url, &api_key, &config)
                .await?;

        // Poll for result with retry
        let markdown_content =
            Self::poll_for_result_with_retry(&client, &job_id, &base_url, &api_key, &config)
                .await?;

        // Write results to disk
        Self::write_results_to_disk(&file_path, &markdown_content, cache_dir).await
    }

    async fn create_parse_job_with_retry(
        client: &reqwest::Client,
        file_path: &str,
        base_url: &str,
        api_key: &str,
        config: &LlamaParseConfig,
    ) -> Result<String, JobError> {
        let file_path = file_path.to_string();
        let base_url = base_url.to_string();
        let api_key = api_key.to_string();
        let parse_kwargs = config.parse_kwargs.clone();
        let client = client.clone();

        let mut last_error = None;

        for attempt in 0..=config.max_retries {
            match Self::create_parse_job(&client, &file_path, &base_url, &api_key, &parse_kwargs)
                .await
            {
                Ok(job_id) => return Ok(job_id),
                Err(JobError::HttpError(err)) => {
                    last_error = Some(err.to_string());

                    // Don't retry on the last attempt
                    if attempt == config.max_retries {
                        return Err(JobError::RetryExhausted(format!(
                            "Job creation failed after {} attempts. Last error: {}",
                            config.max_retries + 1,
                            err
                        )));
                    }

                    // Check if error is retryable
                    let is_retryable = err.is_connect()
                        || err.is_timeout()
                        || err.is_request()
                        || err.to_string().contains("broken pipe")
                        || err.to_string().contains("connection reset")
                        || err.to_string().contains("connection aborted")
                        || err.to_string().contains("network unreachable")
                        || (err.status().map(|s| s.is_server_error()).unwrap_or(false));

                    if !is_retryable {
                        return Err(JobError::HttpError(err));
                    }

                    // Calculate backoff delay
                    let delay = config.retry_delay_ms as f64
                        * config.backoff_multiplier.powi(attempt as i32);
                    let delay_ms = delay as u64;

                    eprintln!(
                        "Job creation failed (attempt {}/{}): {}. Retrying in {}ms...",
                        attempt + 1,
                        config.max_retries + 1,
                        err,
                        delay_ms
                    );

                    sleep(Duration::from_millis(delay_ms)).await;
                }
                Err(other_err) => return Err(other_err), // Don't retry non-HTTP errors
            }
        }

        // This should never be reached due to the logic above, but just in case
        Err(JobError::RetryExhausted(format!(
            "Unexpected retry exhaustion during job creation. Last error: {}",
            last_error.unwrap_or_else(|| "Unknown".to_string())
        )))
    }

    async fn poll_for_result_with_retry(
        client: &reqwest::Client,
        job_id: &str,
        base_url: &str,
        api_key: &str,
        config: &LlamaParseConfig,
    ) -> Result<String, JobError> {
        let job_id = job_id.to_string();
        let base_url = base_url.to_string();
        let api_key = api_key.to_string();
        let client = client.clone();

        let mut last_error = None;

        for attempt in 0..=config.max_retries {
            match Self::poll_for_result(
                &client,
                &job_id,
                &base_url,
                &api_key,
                config.max_timeout,
                config.check_interval,
            )
            .await
            {
                Ok(result) => return Ok(result),
                Err(JobError::HttpError(err)) => {
                    last_error = Some(err.to_string());

                    // Don't retry on the last attempt
                    if attempt == config.max_retries {
                        return Err(JobError::RetryExhausted(format!(
                            "Polling failed after {} attempts. Last error: {}",
                            config.max_retries + 1,
                            err
                        )));
                    }

                    // Check if error is retryable
                    let is_retryable = err.is_connect()
                        || err.is_timeout()
                        || err.is_request()
                        || err.to_string().contains("broken pipe")
                        || err.to_string().contains("connection reset")
                        || err.to_string().contains("connection aborted")
                        || err.to_string().contains("network unreachable")
                        || (err.status().map(|s| s.is_server_error()).unwrap_or(false));

                    if !is_retryable {
                        return Err(JobError::HttpError(err));
                    }

                    // Calculate backoff delay
                    let delay = config.retry_delay_ms as f64
                        * config.backoff_multiplier.powi(attempt as i32);
                    let delay_ms = delay as u64;

                    eprintln!(
                        "Polling failed (attempt {}/{}): {}. Retrying in {}ms...",
                        attempt + 1,
                        config.max_retries + 1,
                        err,
                        delay_ms
                    );

                    sleep(Duration::from_millis(delay_ms)).await;
                }
                Err(JobError::TimeoutError) => {
                    // Timeout errors are not retryable as they indicate the job itself timed out
                    return Err(JobError::TimeoutError);
                }
                Err(other_err) => return Err(other_err), // Don't retry other errors
            }
        }

        // This should never be reached due to the logic above, but just in case
        Err(JobError::RetryExhausted(format!(
            "Unexpected retry exhaustion during polling. Last error: {}",
            last_error.unwrap_or_else(|| "Unknown".to_string())
        )))
    }

    async fn create_parse_job(
        client: &Client,
        file_path: &str,
        base_url: &str,
        api_key: &str,
        parse_kwargs: &HashMap<String, String>,
    ) -> Result<String, JobError> {
        let file_content = fs::read(file_path)?;
        let filename = Path::new(file_path).file_name().unwrap().to_str().unwrap();

        let mime_type = mime_guess::from_path(file_path)
            .first_or_octet_stream()
            .to_string();

        let file_part = multipart::Part::bytes(file_content)
            .file_name(filename.to_string())
            .mime_str(&mime_type)
            .map_err(|e| JobError::InvalidResponse(e.to_string()))?;

        let mut form = multipart::Form::new().part("file", file_part);

        // Add parse kwargs as form data
        for (key, value) in parse_kwargs {
            form = form.text(key.clone(), value.clone());
        }

        let response = client
            .post(format!("{base_url}/api/parsing/upload"))
            .header("Authorization", format!("Bearer {api_key}"))
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(JobError::InvalidResponse(format!(
                "Upload failed: {error_text}"
            )));
        }

        let job_response: JobResponse = response.json().await?;
        Ok(job_response.id)
    }

    async fn poll_for_result(
        client: &Client,
        job_id: &str,
        base_url: &str,
        api_key: &str,
        max_timeout: u64,
        check_interval: u64,
    ) -> Result<String, JobError> {
        let start = SystemTime::now();
        let timeout_duration = Duration::from_secs(max_timeout);

        loop {
            sleep(Duration::from_secs(check_interval)).await;

            // Check if we've timed out
            if start.elapsed().unwrap_or_default() > timeout_duration {
                return Err(JobError::TimeoutError);
            }

            // Check job status
            let status_response = client
                .get(format!("{base_url}/api/parsing/job/{job_id}"))
                .header("Authorization", format!("Bearer {api_key}"))
                .send()
                .await?;

            if !status_response.status().is_success() {
                continue; // Retry on error
            }

            let job_status: JobStatus = status_response.json().await?;

            match job_status.status.as_str() {
                "SUCCESS" => {
                    // Get the result
                    let result_response = client
                        .get(format!(
                            "{base_url}/api/parsing/job/{job_id}/result/markdown"
                        ))
                        .header("Authorization", format!("Bearer {api_key}"))
                        .send()
                        .await?;

                    if !result_response.status().is_success() {
                        return Err(JobError::InvalidResponse(
                            "Failed to get result".to_string(),
                        ));
                    }

                    let job_result: JobResult = result_response.json().await?;
                    return Ok(job_result.markdown);
                }
                "PENDING" => {
                    // Continue polling
                    continue;
                }
                "ERROR" | "CANCELED" => {
                    return Err(JobError::InvalidResponse(format!(
                        "Job failed with status: {}",
                        job_status.status
                    )));
                }
                _ => {
                    return Err(JobError::InvalidResponse(format!(
                        "Unknown status: {}",
                        job_status.status
                    )));
                }
            }
        }
    }

    async fn write_results_to_disk(
        file_path: &str,
        markdown_content: &str,
        cache_dir: PathBuf,
    ) -> Result<String, JobError> {
        let path = Path::new(file_path);
        let filename = path.file_name().unwrap().to_str().unwrap();

        // Write the markdown content
        let parsed_path = cache_dir.join(format!("{filename}.md"));
        fs::write(&parsed_path, markdown_content)?;

        // Write metadata
        let metadata_path = cache_dir.join(format!("{filename}.metadata.json"));
        let file_metadata = fs::metadata(path)?;

        let modified_time = file_metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let metadata = FileMetadata {
            modified_time,
            size: file_metadata.len(),
            parsed_path: parsed_path.to_string_lossy().to_string(),
        };

        fs::write(metadata_path, serde_json::to_string_pretty(&metadata)?)?;

        Ok(parsed_path.to_string_lossy().to_string())
    }
}
