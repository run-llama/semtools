mod llama_parse_backend;

use anyhow::Result;
use clap::Parser;
use std::path::Path;

use llama_parse_backend::{LlamaParseBackend, LlamaParseConfig};

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

    /// Verbose output
    #[clap(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Get config file path
    let config_path = args.parse_config.unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap()
            .join(".parse_config.json")
            .to_string_lossy()
            .to_string()
    });

    // Load configuration
    let config = LlamaParseConfig::from_config_file(&config_path)?;

    if args.verbose {
        eprintln!("Using config file: {}", config_path);
        eprintln!("Backend: {}", args.backend);
        eprintln!("Processing {} files", args.files.len());
    }

    // Validate that files exist
    for file in &args.files {
        if !Path::new(file).exists() {
            eprintln!("Warning: File does not exist: {}", file);
        }
    }

    // Create backend and process files
    match args.backend.as_str() {
        "llama-parse" => {
            let backend = LlamaParseBackend::new(config)?;
            let results = backend.parse(args.files).await?;
            
            // Output the paths to parsed files, one per line
            for result_path in results {
                println!("{}", result_path);
            }
        }
        _ => {
            eprintln!("Error: Unknown backend '{}'. Supported backends: llama-parse", args.backend);
            std::process::exit(1);
        }
    }

    Ok(())
}
