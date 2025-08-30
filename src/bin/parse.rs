use anyhow::Result;
use clap::Parser;
use std::path::Path;

use semtools::{
    LMStudioBackend, LMStudioConfig, LlamaParseBackend, LlamaParseConfig, ParseBackend,
};

#[derive(Parser, Debug)]
#[command(version, about = "A CLI tool for parsing documents using various backends", long_about = None)]
struct Args {
    /// Path to the config file. Defaults to ~/.parse_config.json
    #[clap(short = 'c', long)]
    parse_config: Option<String>,

    /// The backend type to use for parsing. Defaults to `llama-parse`
    #[clap(short, long, default_value = "llama-parse")]
    backend: String,

    /// Files to parse
    #[clap(required = true)]
    files: Vec<String>,

    /// Verbose output while parsing
    #[clap(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Get config file path based on backend
    let config_path = args.parse_config.unwrap_or_else(|| {
        let config_filename = match args.backend.as_str() {
            "lmstudio" => ".lmstudio_parse_config.json",
            _ => ".parse_config.json",
        };
        dirs::home_dir()
            .unwrap()
            .join(config_filename)
            .to_string_lossy()
            .to_string()
    });

    // Show config info in verbose mode
    if args.verbose {
        eprintln!("📁 Looking for config: {}", config_path);
        if !std::path::Path::new(&config_path).exists() {
            eprintln!("ℹ️  Config file not found, will try fallbacks and defaults");
        }
    }

    // Validate that files exist
    for file in &args.files {
        if !Path::new(file).exists() {
            eprintln!("Warning: File does not exist: {file}");
        }
    }

    // Create backend and process files
    let backend: Box<dyn ParseBackend> = match args.backend.as_str() {
        "llama-parse" => {
            let config = LlamaParseConfig::from_config_file(&config_path)
                .map_err(|e| anyhow::Error::msg(format!(
                    "Failed to load LlamaParse config: {}\n💡 Tip: Set LLAMA_CLOUD_API_KEY environment variable or create {}", 
                    e, config_path
                )))?;
            Box::new(LlamaParseBackend::new(config, args.verbose)?)
        }
        "lmstudio" => {
            let config = LMStudioConfig::from_config_file(&config_path)
                .map_err(|e| anyhow::Error::msg(format!(
                    "Failed to load LMStudio config: {}\n💡 Tip: Create {} or set LMSTUDIO_BASE_URL/LMSTUDIO_MODEL environment variables", 
                    e, config_path
                )))?;
            Box::new(LMStudioBackend::new(config, args.verbose)?)
        }
        _ => {
            eprintln!(
                "Error: Unknown backend '{}'. Supported backends: llama-parse, lmstudio",
                args.backend
            );
            std::process::exit(1);
        }
    };

    // Process files
    let results = backend
        .parse(args.files)
        .await
        .map_err(|e| anyhow::Error::msg(format!("Parse error: {}", e)))?;

    // Output the paths to parsed files, one per line
    for result_path in results {
        println!("{result_path}");
    }

    Ok(())
}
