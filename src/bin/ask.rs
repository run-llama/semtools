use anyhow::Result;
use async_openai::Client;
use async_openai::config::OpenAIConfig;
use clap::Parser;
use model2vec_rs::model::StaticModel;

use semtools::SemtoolsConfig;
use semtools::ask::chat_agent::ask_agent;
use semtools::ask::responses_agent::ask_agent_responses;
use semtools::config::ApiMode;
use semtools::search::MODEL_NAME;

#[derive(Parser, Debug)]
#[command(version, about = "A CLI tool for fast semantic keyword search", long_about = None)]
struct Args {
    /// Query to prompt the agent with
    query: String,

    /// Files to search (positional arguments, optional if using stdin)
    #[arg(help = "Files to search, optional if using stdin")]
    files: Vec<String>,

    /// Path to the config file. Defaults to ~/.semtools_config.json
    #[clap(short = 'c', long)]
    config: Option<String>,

    /// OpenAI API key (overrides config file and env var)
    #[clap(long)]
    api_key: Option<String>,

    /// OpenAI base URL (overrides config file)
    #[clap(long)]
    base_url: Option<String>,

    /// Model to use for the agent (overrides config file)
    #[clap(short, long)]
    model: Option<String>,

    /// API mode to use: 'chat' or 'responses' (overrides config file)
    #[clap(long)]
    api_mode: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Load configuration
    let config_path = args
        .config
        .unwrap_or_else(|| SemtoolsConfig::default_config_path());
    let semtools_config = SemtoolsConfig::from_config_file(&config_path)?;
    let ask_config = semtools_config.ask.unwrap_or_default();

    // Resolve API key with priority: CLI arg > config file > env var > error
    let api_key = args
        .api_key
        .or(ask_config.api_key)
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "OpenAI API key not found. Set via --api-key, config file, or OPENAI_API_KEY env var"
            )
        })?;

    // Resolve base URL with priority: CLI arg > config file > default
    let base_url = args.base_url.or(ask_config.base_url);

    // Resolve model with priority: CLI arg > config file > default
    let model_name = args
        .model
        .or(ask_config.model)
        .unwrap_or_else(|| "gpt-4o-mini".to_string());

    // Resolve max iterations from config
    let max_iterations = ask_config.max_iterations;

    // Resolve API mode with priority: CLI arg > config file > default
    let api_mode = if let Some(mode_str) = args.api_mode {
        match mode_str.to_lowercase().as_str() {
            "chat" => ApiMode::Chat,
            "responses" => ApiMode::Responses,
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid API mode: '{}'. Must be 'chat' or 'responses'",
                    mode_str
                ));
            }
        }
    } else {
        ask_config.api_mode
    };

    // Load embedding model
    let model = StaticModel::from_pretrained(
        MODEL_NAME, // "minishlab/potion-multilingual-128M",
        None,       // Optional: Hugging Face API token for private models
        None, // Optional: bool to override model's default normalization. `None` uses model's config.
        None, // Optional: subfolder if model files are not at the root of the repo/path
    )?;

    // Create OpenAI client
    let mut openai_config = OpenAIConfig::default().with_api_key(api_key);
    if let Some(url) = base_url {
        openai_config = openai_config.with_api_base(url);
    }
    let client = Client::with_config(openai_config);

    // Run the appropriate agent based on API mode
    let response = match api_mode {
        ApiMode::Chat => {
            ask_agent(
                args.files,
                &args.query,
                &model,
                &client,
                &model_name,
                max_iterations,
            )
            .await?
        }
        ApiMode::Responses => {
            ask_agent_responses(
                args.files,
                &args.query,
                &model,
                &client,
                &model_name,
                max_iterations,
            )
            .await?
        }
    };

    println!("\n{}", response);

    Ok(())
}
