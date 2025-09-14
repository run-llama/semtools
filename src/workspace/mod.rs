use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

pub mod store;

pub use store::WorkspaceStats;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub name: String,
    pub root_dir: String,         // e.g., ~/.semtools/my-workspace
    pub doc_top_k: usize,         // default 250
    pub in_batch_size: usize,     // default 5_000
    pub oversample_factor: usize, // default 3
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            root_dir: String::new(),
            doc_top_k: 250,
            in_batch_size: 5_000,
            oversample_factor: 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub config: WorkspaceConfig,
}

impl Workspace {
    pub fn open() -> Result<Self> {
        let active_workspace = Self::active()?;
        let cfg_path = Self::config_path_for(&active_workspace)?;
        let cfg = std::fs::read_to_string(&cfg_path)
            .ok()
            .and_then(|s| serde_json::from_str::<WorkspaceConfig>(&s).ok());
        let mut config = cfg.unwrap_or_default();
        if config.root_dir.is_empty() {
            config.root_dir = Self::root_path(&active_workspace)?;
        }
        if config.name.is_empty() || config.name == "default" {
            config.name = active_workspace;
        }
        Ok(Self { config })
    }

    pub fn save(&self) -> Result<()> {
        let cfg_path = Self::config_path_for(&self.config.name)?;
        let parent = std::path::Path::new(&cfg_path).parent().unwrap();
        std::fs::create_dir_all(parent)?;
        let s = serde_json::to_string_pretty(&self.config)?;
        std::fs::write(cfg_path, s)?;
        Ok(())
    }


    pub fn active_path() -> Result<String> {
        let active = std::env::var("SEMTOOLS_WORKSPACE").unwrap_or_default();
        if active.is_empty() {
            bail!("No active workspace. Run: workspace select <name>");
        }
        Self::root_path(&active)
    }

    pub fn active() -> Result<String> {
        let active = std::env::var("SEMTOOLS_WORKSPACE").unwrap_or_default();
        if active.is_empty() {
            bail!("No active workspace. Run: workspace select <name>");
        }
        Ok(active)
    }
}

impl Workspace {
    pub fn root_path(name: &str) -> Result<String> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("No home dir found?"))?;
        Ok(home
            .join(".semtools")
            .join("workspaces")
            .join(name)
            .to_string_lossy()
            .to_string())
    }

    fn config_path_for(name: &str) -> Result<String> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("No home dir found?"))?;
        Ok(home
            .join(".semtools")
            .join("workspaces")
            .join(name)
            .join("config.json")
            .to_string_lossy()
            .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_workspace_config_default() {
        let config = WorkspaceConfig::default();

        assert_eq!(config.name, "default");
        assert_eq!(config.root_dir, "");
        assert_eq!(config.doc_top_k, 250);
        assert_eq!(config.in_batch_size, 5_000);
        assert_eq!(config.oversample_factor, 3);
    }

    #[test]
    fn test_workspace_config_serialization() {
        let config = WorkspaceConfig {
            name: "test-workspace".to_string(),
            root_dir: "/tmp/test".to_string(),
            doc_top_k: 100,
            in_batch_size: 1000,
            oversample_factor: 2,
        };

        // Test serialization
        let json = serde_json::to_string(&config).expect("Failed to serialize");
        assert!(json.contains("test-workspace"));
        assert!(json.contains("100"));

        // Test deserialization
        let deserialized: WorkspaceConfig =
            serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(deserialized.name, config.name);
        assert_eq!(deserialized.root_dir, config.root_dir);
        assert_eq!(deserialized.doc_top_k, config.doc_top_k);
        assert_eq!(deserialized.in_batch_size, config.in_batch_size);
        assert_eq!(deserialized.oversample_factor, config.oversample_factor);
    }

    #[test]
    fn test_workspace_set_and_get_active() {
        // Test setting active workspace
        unsafe { std::env::set_var("SEMTOOLS_WORKSPACE", "test-workspace"); }

        // Test getting active workspace
        let active = Workspace::active().expect("Failed to get active");
        assert_eq!(active, "test-workspace");

        // Test getting active path
        let active_path = Workspace::active_path().expect("Failed to get active path");
        assert!(active_path.contains("test-workspace"));
    }

    #[test]
    fn test_workspace_active_no_workspace() {
        // Save current state
        let original = std::env::var("SEMTOOLS_WORKSPACE").ok();

        // Clear environment variable
        unsafe {
            std::env::remove_var("SEMTOOLS_WORKSPACE");
        }

        // Should fail when no active workspace
        let result = Workspace::active();
        assert!(result.is_err());

        let result = Workspace::active_path();
        assert!(result.is_err());

        // Restore original state
        if let Some(value) = original {
            unsafe { std::env::set_var("SEMTOOLS_WORKSPACE", value); }
        }
    }

    #[test]
    fn test_workspace_root_path() {
        let path = Workspace::root_path("my-workspace").expect("Failed to get root path");

        assert!(path.contains(".semtools"));
        assert!(path.contains("workspaces"));
        assert!(path.contains("my-workspace"));
    }

    #[test]
    fn test_workspace_config_path() {
        let path = Workspace::config_path_for("my-workspace").expect("Failed to get config path");

        assert!(path.contains(".semtools"));
        assert!(path.contains("workspaces"));
        assert!(path.contains("my-workspace"));
        assert!(path.ends_with("config.json"));
    }

    #[test]
    fn test_workspace_save_and_open() {
        let workspace_name = "test-save-open";

        // Save current state
        let original = std::env::var("SEMTOOLS_WORKSPACE").ok();

        // Set up workspace
        unsafe { std::env::set_var("SEMTOOLS_WORKSPACE", workspace_name); }

        let workspace = Workspace {
            config: WorkspaceConfig {
                name: workspace_name.to_string(),
                root_dir: Workspace::root_path(workspace_name).expect("Failed to get root path"),
                doc_top_k: 123,
                in_batch_size: 456,
                oversample_factor: 7,
            },
        };

        // Save the workspace
        workspace.save().expect("Failed to save workspace");

        // Verify config file was created
        let config_path =
            Workspace::config_path_for(workspace_name).expect("Failed to get config path");
        assert!(std::path::Path::new(&config_path).exists());

        // Read and verify config file content
        let config_content = fs::read_to_string(&config_path).expect("Failed to read config");
        assert!(config_content.contains("test-save-open"));
        assert!(config_content.contains("123"));

        // Test loading the config manually (since Workspace::open() depends on environment)
        let loaded_config: WorkspaceConfig =
            serde_json::from_str(&config_content).expect("Failed to parse saved config");

        assert_eq!(loaded_config.name, workspace.config.name);
        assert_eq!(loaded_config.doc_top_k, workspace.config.doc_top_k);
        assert_eq!(loaded_config.in_batch_size, workspace.config.in_batch_size);
        assert_eq!(
            loaded_config.oversample_factor,
            workspace.config.oversample_factor
        );

        // Clean up - remove the test config file
        let _ = fs::remove_file(&config_path);
        let _ = fs::remove_dir_all(std::path::Path::new(&config_path).parent().unwrap());

        // Restore original state
        if let Some(value) = original {
            unsafe { std::env::set_var("SEMTOOLS_WORKSPACE", value); }
        }
    }

    #[test]
    fn test_workspace_open_with_defaults() {
        let workspace_name = "test-defaults";

        // Save current state
        let original = std::env::var("SEMTOOLS_WORKSPACE").ok();

        unsafe { std::env::set_var("SEMTOOLS_WORKSPACE", workspace_name); }

        // Since we haven't saved a config file, open should use defaults
        let workspace = Workspace::open().expect("Failed to open workspace");

        assert_eq!(workspace.config.name, workspace_name);
        assert!(!workspace.config.root_dir.is_empty());
        assert_eq!(workspace.config.doc_top_k, 250); // Default value

        // Restore original state
        if let Some(value) = original {
            unsafe { std::env::set_var("SEMTOOLS_WORKSPACE", value); }
        }
    }
}
