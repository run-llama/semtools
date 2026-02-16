use anyhow::{Context, Result};

#[cfg(feature = "workspace")]
use crate::workspace::{Workspace, WorkspaceConfig, store::Store};

use crate::json_mode::{PruneOutput, WorkspaceOutput};

#[cfg(not(feature = "workspace"))]
use crate::json_mode::ErrorOutput;

pub async fn workspace_use_cmd(name: String, json: bool) -> Result<()> {
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

        if json {
            // Try to get document count from store, or use 0 for new workspace
            let total_documents = if let Ok(store) = Store::open(&ws.config.root_dir) {
                if let Ok(stats) = store.get_stats() {
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
        if json {
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
    Ok(())
}

pub async fn workspace_status_cmd(json: bool, workspace_name: Option<&str>) -> Result<()> {
    #[cfg(feature = "workspace")]
    {
        let _name = Workspace::active(workspace_name).context("No active workspace")?;
        let ws = Workspace::open(workspace_name)?;

        // Open store and get stats
        let store = Store::open(&ws.config.root_dir)?;
        let stats = store.get_stats()?;

        if json {
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
        if json {
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
    Ok(())
}

pub async fn workspace_prune_cmd(json: bool, workspace_name: Option<&str>) -> Result<()> {
    #[cfg(feature = "workspace")]
    {
        let _name = Workspace::active(workspace_name).context("No active workspace")?;
        let ws = Workspace::open(workspace_name)?;
        let store = Store::open(&ws.config.root_dir)?;

        // Get all document paths from the workspace
        let all_paths = store.get_all_document_paths()?;
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
            store.delete_documents(&missing_paths)?;
        }

        if json {
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
        if json {
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
    Ok(())
}
