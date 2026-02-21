use anyhow::{Context, Result};
use glob::glob;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Returns the base Claude projects directory: ~/.claude/projects/
pub fn claude_projects_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    let path = home.join(".claude").join("projects");
    Ok(path)
}

/// Discovers all sessions-index.json files under ~/.claude/projects/
pub fn discover_session_indices() -> Result<Vec<PathBuf>> {
    let projects_dir = claude_projects_dir()?;
    let pattern = projects_dir
        .join("*")
        .join("sessions-index.json")
        .to_string_lossy()
        .to_string();

    let mut indices = Vec::new();
    for entry in glob(&pattern).context("Failed to glob session indices")? {
        match entry {
            Ok(path) => indices.push(path),
            Err(e) => {
                log::warn!("Error reading glob entry: {}", e);
            }
        }
    }

    Ok(indices)
}

/// Decodes an encoded project path from the directory name.
/// Claude Code encodes paths like: `-Users-username-project` for `/Users/username/project`
pub fn decode_project_path(encoded: &str) -> String {
    // The encoding replaces '/' with '-' and prefixes with '-'
    // e.g., "-Users-madzarmaksim-Workspace-experiments" -> "/Users/madzarmaksim/Workspace/experiments"
    if encoded.starts_with('-') {
        encoded.replacen('-', "/", 1).replace('-', "/")
    } else {
        encoded.replace('-', "/")
    }
}

/// Gets the project directory path from a sessions-index.json path
pub fn project_dir_from_index(index_path: &Path) -> Option<PathBuf> {
    index_path.parent().map(|p| p.to_path_buf())
}

/// Extracts the encoded project name from a sessions-index.json path
pub fn encoded_project_name(index_path: &Path) -> Option<String> {
    index_path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
}

/// Launches `claude --resume <session_id>` replacing the current process
pub fn resume_session(session_id: &str) -> Result<()> {
    let status = Command::new("claude")
        .arg("--resume")
        .arg(session_id)
        .status()
        .context("Failed to launch 'claude --resume'. Is Claude Code installed?")?;

    if !status.success() {
        anyhow::bail!("claude --resume exited with status: {}", status);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_project_path() {
        assert_eq!(
            decode_project_path("-Users-user-project"),
            "/Users/user/project"
        );
    }

    #[test]
    fn test_decode_project_path_no_prefix() {
        assert_eq!(decode_project_path("tmp-project"), "tmp/project");
    }
}
