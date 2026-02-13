use clap::{Parser, Subcommand};
use semtools::cmds::ask::ask_cmd;
use semtools::cmds::parse::parse_cmd;
use semtools::cmds::search::search_cmd;
use semtools::cmds::workspace::{workspace_prune_cmd, workspace_status_cmd, workspace_use_cmd};

#[derive(Parser, Debug)]
struct SemtoolsArgs {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug)]
enum WorkspaceCommands {
    /// Use or create a workspace (prints export command to run)
    Use { name: String },
    /// Show active workspace and basic stats
    Status,
    /// Remove stale or missing files from store
    Prune {},
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[cfg(feature = "parse")]
    /// A CLI tool for parsing documents using various backends
    Parse {
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
    },
    #[cfg(feature = "search")]
    /// A CLI tool for fast semantic keyword search
    Search {
        /// Query to search for (positional argument)
        query: String,

        /// Files to search (positional arguments, optional if using stdin)
        #[arg(help = "Files to search, optional if using stdin")]
        files: Vec<String>,

        /// How many lines before/after to return as context
        #[arg(short = 'n', long = "n-lines", alias = "context", default_value_t = 3)]
        n_lines: usize,

        /// The top-k files or texts to return (ignored if max_distance is set)
        #[arg(long, default_value_t = 3)]
        top_k: usize,

        /// Return all results with distance below this threshold (0.0+)
        #[arg(short = 'm', long = "max-distance", alias = "threshold")]
        max_distance: Option<f64>,

        /// Perform case-insensitive search (default is false)
        #[arg(short, long, default_value_t = false)]
        ignore_case: bool,

        /// Output results in JSON format
        #[clap(short, long)]
        json: bool,
    },
    #[cfg(feature = "ask")]
    /// A CLI tool for document-based question-answering
    Ask {
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

        /// Output results in JSON or text format
        #[clap(short, long)]
        json: bool,
    },
    #[cfg(feature = "workspace")]
    /// Manage semtools workspaces
    Workspace {
        /// Output results in JSON format
        #[clap(short, long, global = true)]
        json: bool,

        #[command(subcommand)]
        command: WorkspaceCommands,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = SemtoolsArgs::parse();
    match args.cmd {
        Commands::Ask {
            query,
            files,
            config,
            api_key,
            base_url,
            model,
            api_mode,
            json,
        } => {
            ask_cmd(
                query, files, config, api_key, base_url, model, api_mode, json,
            )
            .await?;
        }
        Commands::Parse {
            config,
            backend,
            files,
            verbose,
        } => {
            parse_cmd(config, backend, files, verbose).await?;
        }
        Commands::Search {
            query,
            files,
            n_lines,
            top_k,
            max_distance,
            ignore_case,
            json,
        } => {
            search_cmd(
                query,
                files,
                n_lines,
                top_k,
                max_distance,
                ignore_case,
                json,
            )
            .await?;
        }
        Commands::Workspace { json, command } => match command {
            WorkspaceCommands::Use { name } => {
                workspace_use_cmd(name, json).await?;
            }
            WorkspaceCommands::Prune {} => {
                workspace_prune_cmd(json).await?;
            }
            WorkspaceCommands::Status => {
                workspace_status_cmd(json).await?;
            }
        },
    }

    Ok(())
}
