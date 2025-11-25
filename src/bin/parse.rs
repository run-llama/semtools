use anyhow::Result;
use clap::Parser;
use std::path::Path;

use semtools::{LlamaParseBackend, SemtoolsConfig};

#[derive(Parser, Debug)]
#[command(version, about = "A CLI tool for parsing documents using various backends", long_about = None)]
struct Args {
    /// Path to the config file. Defaults to ~/.semtools_config.json
    #[clap(short = 'c', long)]
    config: Option<String>,

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

    // Get config file path
    let config_path = args
        .config
        .unwrap_or_else(|| SemtoolsConfig::default_config_path());

    // Load configuration
    let semtools_config = SemtoolsConfig::from_config_file(&config_path)?;
    let parse_config = semtools_config.parse.unwrap_or_default();

    // Validate that files exist
    for file in &args.files {
        if !Path::new(file).exists() {
            eprintln!("Warning: File does not exist: {file}");
        }
    }

    // Create backend and process files
    match args.backend.as_str() {
        "llama-parse" => {
            let backend = LlamaParseBackend::new(parse_config, args.verbose)?;
            let results = backend.parse(args.files).await?;

            // Output the paths to parsed files, one per line
            for result_path in results {
                println!("{result_path}");
            }
        }
        _ => {
            eprintln!(
                "Error: Unknown backend '{}'. Supported backends: llama-parse",
                args.backend
            );
            std::process::exit(1);
        }
    }

    Ok(())
}
