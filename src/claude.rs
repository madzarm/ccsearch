use anyhow::{Context, Result};
use glob::glob;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Returns the base Claude projects directory: ~/.claude/projects/
pub fn claude_projects_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    let path = home.join(".claude").join("projects");
    Ok(path)
}

/// Returns the path to ~/.claude/history.jsonl
pub fn history_jsonl_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".claude").join("history.jsonl"))
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

/// Discovers all .jsonl session files under ~/.claude/projects/ directly.
/// Returns a map of session_id -> (jsonl_path, project_dir_encoded_name)
pub fn discover_all_session_files() -> Result<HashMap<String, (PathBuf, String)>> {
    let projects_dir = claude_projects_dir()?;
    let pattern = projects_dir
        .join("*")
        .join("*.jsonl")
        .to_string_lossy()
        .to_string();

    let mut sessions = HashMap::new();
    for entry in glob(&pattern).context("Failed to glob session files")? {
        match entry {
            Ok(path) => {
                let filename = match path.file_stem().and_then(|s| s.to_str()) {
                    Some(f) => f.to_string(),
                    None => continue,
                };
                // Skip non-session files (agent files start with "agent-")
                if filename.starts_with("agent-") {
                    continue;
                }
                // Session IDs are UUIDs — quick sanity check
                if filename.len() < 32 || !filename.contains('-') {
                    continue;
                }
                let encoded_name = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                sessions.insert(filename, (path, encoded_name));
            }
            Err(e) => {
                log::warn!("Error reading glob entry: {}", e);
            }
        }
    }

    Ok(sessions)
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

/// Launches `claude --resume <session_id>` from the session's project directory.
/// Claude Code needs to be run from the correct project dir to find the session.
pub fn resume_session(session_id: &str, project_path: Option<&str>) -> Result<()> {
    let mut cmd = Command::new("claude");
    cmd.arg("--resume").arg(session_id);

    // Set working directory to the project path if available
    if let Some(proj) = project_path {
        let proj_path = Path::new(proj);
        if proj_path.is_dir() {
            cmd.current_dir(proj_path);
        }
    }

    let status = cmd
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
