use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[cfg(feature = "workspace")]
use semtools::workspace::{Workspace, WorkspaceConfig, store::Store};

use semtools::json_mode::{PruneOutput, WorkspaceOutput};

#[cfg(not(feature = "workspace"))]
use semtools::json_mode::ErrorOutput;

#[derive(Parser, Debug)]
#[command(version, about = "Manage semtools workspaces", long_about = None)]
struct Args {
    /// Output results in JSON format
    #[clap(short, long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Use or create a workspace (prints export command to run)
    Use { name: String },
    /// Show active workspace and basic stats
    Status,
    /// Remove stale or missing files from store
    Prune {},
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Use { name } => {
            #[cfg(feature = "workspace")]
            {
                // Initialize new workspace configuration
                let ws = Workspace {
                    config: WorkspaceConfig {
                        name: name.clone(),
                        root_dir: Workspace::root_path(&name)?,
                        ..Default::default()
                    },
                };
                ws.save()?;

                if args.json {
                    // Try to get document count from store, or use 0 for new workspace
                    let total_documents = if let Ok(store) = Store::open(&ws.config.root_dir).await
                    {
                        if let Ok(stats) = store.get_stats().await {
                            stats.total_documents
                        } else {
                            0
                        }
                    } else {
                        0
                    };

                    let output = WorkspaceOutput {
                        name: ws.config.name.clone(),
                        root_dir: ws.config.root_dir.clone(),
                        total_documents,
                    };
                    let json_output = serde_json::to_string_pretty(&output)?;
                    println!("{}", json_output);
                } else {
                    println!("Workspace '{name}' configured.");
                    println!("To activate it, run:");
                    println!("  export SEMTOOLS_WORKSPACE={name}");
                    println!();
                    println!("Or add this to your shell profile (.bashrc, .zshrc, etc.)");
                }
            }
            #[cfg(not(feature = "workspace"))]
            {
                if args.json {
                    let error_output = ErrorOutput {
                        error: "workspace feature not enabled".to_string(),
                        error_type: "FeatureNotEnabled".to_string(),
                    };
                    let json_output = serde_json::to_string_pretty(&error_output)?;
                    eprintln!("{}", json_output);
                } else {
                    println!("workspace feature not enabled");
                }
            }
        }
        Commands::Status => {
            #[cfg(feature = "workspace")]
            {
                let _name = Workspace::active().context("No active workspace")?;
                let ws = Workspace::open()?;

                // Open store and get stats
                let store = Store::open(&ws.config.root_dir).await?;
                let stats = store.get_stats().await?;

                if args.json {
                    let output = WorkspaceOutput {
                        name: ws.config.name.clone(),
                        root_dir: ws.config.root_dir.clone(),
                        total_documents: stats.total_documents,
                    };
                    let json_output = serde_json::to_string_pretty(&output)?;
                    println!("{}", json_output);
                } else {
                    println!("Active workspace: {}", ws.config.name);
                    println!("Root: {}", ws.config.root_dir);
                    println!("Documents: {}", stats.total_documents);
                    if stats.has_index {
                        let index_info = stats.index_type.unwrap_or_else(|| "Unknown".to_string());
                        println!("Index: Yes ({index_info})");
                    } else {
                        println!("Index: No");
                    }
                }
            }
            #[cfg(not(feature = "workspace"))]
            {
                if args.json {
                    let error_output = ErrorOutput {
                        error: "workspace feature not enabled".to_string(),
                        error_type: "FeatureNotEnabled".to_string(),
                    };
                    let json_output = serde_json::to_string_pretty(&error_output)?;
                    eprintln!("{}", json_output);
                } else {
                    println!("workspace feature not enabled");
                }
            }
        }
        Commands::Prune {} => {
            #[cfg(feature = "workspace")]
            {
                let _name = Workspace::active().context("No active workspace")?;
                let ws = Workspace::open()?;
                let store = Store::open(&ws.config.root_dir).await?;

                // Get all document paths from the workspace
                let all_paths = store.get_all_document_paths().await?;
                let total_before = all_paths.len();

                // Check which files no longer exist
                let mut missing_paths = Vec::new();
                for path in &all_paths {
                    if !std::path::Path::new(path).exists() {
                        missing_paths.push(path.clone());
                    }
                }

                let files_removed = missing_paths.len();
                let files_remaining = total_before - files_removed;

                if !missing_paths.is_empty() {
                    // Remove stale documents
                    store.delete_documents(&missing_paths).await?;
                }

                if args.json {
                    let output = PruneOutput {
                        files_removed,
                        files_remaining,
                    };
                    let json_output = serde_json::to_string_pretty(&output)?;
                    println!("{}", json_output);
                } else if missing_paths.is_empty() {
                    println!("No stale documents found. Workspace is clean.");
                } else {
                    println!("Found {} stale documents:", missing_paths.len());
                    for path in &missing_paths {
                        println!("  - {path}");
                    }
                    println!(
                        "Removed {} stale documents from workspace.",
                        missing_paths.len()
                    );
                }
            }
            #[cfg(not(feature = "workspace"))]
            {
                if args.json {
                    let error_output = ErrorOutput {
                        error: "workspace feature not enabled".to_string(),
                        error_type: "FeatureNotEnabled".to_string(),
                    };
                    let json_output = serde_json::to_string_pretty(&error_output)?;
                    eprintln!("{}", json_output);
                } else {
                    println!("workspace feature not enabled");
                }
            }
        }
    }

    Ok(())
}
