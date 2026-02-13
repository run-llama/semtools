use anyhow::Result;
use std::path::Path;

use crate::{LlamaParseBackend, SemtoolsConfig};

pub async fn parse_cmd(
    config: Option<String>,
    backend: String,
    files: Vec<String>,
    verbose: bool,
) -> Result<()> {
    // Get config file path
    let config_path = config.unwrap_or_else(SemtoolsConfig::default_config_path);

    // Load configuration
    let semtools_config = SemtoolsConfig::from_config_file(&config_path)?;
    let parse_config = semtools_config.parse.unwrap_or_default();

    // Validate that files exist
    for file in &files {
        if !Path::new(file).exists() {
            eprintln!("Warning: File does not exist: {file}");
        }
    }

    // Create backend and process files
    match backend.as_str() {
        "llama-parse" => {
            let backend = LlamaParseBackend::new(parse_config, verbose)?;
            let results = backend.parse(files).await?;

            // Output the paths to parsed files, one per line
            for result_path in results {
                println!("{result_path}");
            }
        }
        _ => {
            eprintln!(
                "Error: Unknown backend '{}'. Supported backends: llama-parse",
                backend
            );
            std::process::exit(1);
        }
    }

    Ok(())
}
