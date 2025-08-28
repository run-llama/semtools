use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::parse::error::JobError;

#[derive(Debug, Serialize, Deserialize)]
pub struct FileMetadata {
    pub modified_time: u64,
    pub size: u64,
    pub parsed_path: String,
}

pub struct CacheManager {
    pub cache_dir: PathBuf,
}

impl CacheManager {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    pub fn should_skip_file(&self, file_path: &str) -> bool {
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

    pub async fn get_cached_result(&self, file_path: &str) -> Result<String, JobError> {
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

    pub fn get_file_metadata(&self, file_path: &str) -> Result<FileMetadata, JobError> {
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

    pub fn get_metadata_path(&self, file_path: &str) -> PathBuf {
        let path = Path::new(file_path);
        let filename = path.file_name().unwrap().to_str().unwrap();
        self.cache_dir.join(format!("{filename}.metadata.json"))
    }

    pub async fn write_results_to_disk(
        &self,
        file_path: &str,
        markdown_content: &str,
    ) -> Result<String, JobError> {
        let path = Path::new(file_path);
        let filename = path.file_name().unwrap().to_str().unwrap();

        // Write the markdown content
        let parsed_path = self.cache_dir.join(format!("{filename}.md"));
        fs::write(&parsed_path, markdown_content)?;

        // Write metadata
        let metadata_path = self.cache_dir.join(format!("{filename}.metadata.json"));
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
