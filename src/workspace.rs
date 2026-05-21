//! Workspace resolution via `cargo metadata`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;

/// A resolved workspace.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// Absolute path of the workspace root.
    pub root: PathBuf,
    /// Member crates, ordered by name.
    pub crates: Vec<CrateInfo>,
}

/// A single crate within a workspace.
#[derive(Debug, Clone)]
pub struct CrateInfo {
    pub name: String,
    /// Absolute path of the crate's `src/` directory.
    pub src_root: PathBuf,
}

/// Resolve the workspace that contains `workspace_dir`.
pub fn resolve(workspace_dir: &Path) -> Result<Workspace> {
    let metadata = MetadataCommand::new()
        .current_dir(workspace_dir)
        .exec()
        .with_context(|| {
            format!(
                "failed to read cargo metadata for {}",
                workspace_dir.display()
            )
        })?;

    let mut crates: Vec<CrateInfo> = metadata
        .workspace_packages()
        .into_iter()
        .map(|package| {
            let manifest_dir = package
                .manifest_path
                .parent()
                .map(|dir| dir.as_std_path().to_path_buf())
                .unwrap_or_default();
            CrateInfo {
                name: package.name.to_string(),
                src_root: manifest_dir.join("src"),
            }
        })
        .collect();
    crates.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Workspace {
        root: metadata.workspace_root.as_std_path().to_path_buf(),
        crates,
    })
}
