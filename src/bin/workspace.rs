use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[cfg(feature = "workspace")]
use semtools::workspace::{Workspace, WorkspaceConfig, store::Store};

#[derive(Parser, Debug)]
#[command(version, about = "Manage semtools workspaces", long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Select or create a workspace (prints export command to run)
    Select {
        name: String,
    },
    /// Show active workspace and basic stats
    Status,
    /// Remove stale or missing files from store
    Prune {},
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Select { name } => {
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

                println!("Workspace '{name}' configured.");
                println!("To activate it, run:");
                println!("  export SEMTOOLS_WORKSPACE={name}");
                println!();
                println!("Or add this to your shell profile (.bashrc, .zshrc, etc.)");
            }
            #[cfg(not(feature = "workspace"))]
            {
                println!("workspace feature not enabled");
            }
        }
        Commands::Status => {
            #[cfg(feature = "workspace")]
            {
                let _name = Workspace::active().context("No active workspace")?;
                let ws = Workspace::open()?;
                println!("Active workspace: {}", ws.config.name);
                println!("Root: {}", ws.config.root_dir);

                // Open store and print counts/index status
                let store = Store::open(&ws.config.root_dir).await?;
                let stats = store.get_stats().await?;

                println!("Documents: {}", stats.total_documents);
                if stats.has_index {
                    let index_info = stats.index_type.unwrap_or_else(|| "Unknown".to_string());
                    println!("Index: Yes ({index_info})");
                } else {
                    println!("Index: No");
                }
            }
            #[cfg(not(feature = "workspace"))]
            {
                println!("workspace feature not enabled");
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

                // Check which files no longer exist
                let mut missing_paths = Vec::new();
                for path in &all_paths {
                    if !std::path::Path::new(path).exists() {
                        missing_paths.push(path.clone());
                    }
                }

                if missing_paths.is_empty() {
                    println!("No stale documents found. Workspace is clean.");
                } else {
                    println!("Found {} stale documents:", missing_paths.len());
                    for path in &missing_paths {
                        println!("  - {path}");
                    }

                    // Remove stale documents
                    store.delete_documents(&missing_paths).await?;
                    println!(
                        "Removed {} stale documents from workspace.",
                        missing_paths.len()
                    );
                }
            }
            #[cfg(not(feature = "workspace"))]
            {
                println!("workspace feature not enabled");
            }
        }
    }

    Ok(())
}
