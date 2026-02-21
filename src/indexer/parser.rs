use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Top-level structure of sessions-index.json
#[derive(Debug, Deserialize)]
pub struct SessionIndex {
    #[serde(default)]
    pub entries: Vec<SessionIndexEntry>,
}

/// Represents a single entry in sessions-index.json
#[derive(Debug, Deserialize)]
pub struct SessionIndexEntry {
    #[serde(rename = "sessionId")]
    pub session_id: String,

    #[serde(rename = "fullPath", default)]
    pub full_path: Option<String>,

    #[serde(rename = "firstPrompt", default)]
    pub first_prompt: Option<String>,

    #[serde(default)]
    pub summary: Option<String>,

    #[serde(default)]
    pub slug: Option<String>,

    #[serde(rename = "projectPath", default)]
    pub project_path: Option<String>,

    #[serde(rename = "messageCount", default)]
    pub message_count: Option<usize>,

    #[serde(default)]
    pub created: Option<String>,

    #[serde(default)]
    pub modified: Option<String>,

    #[serde(rename = "createdAt", default)]
    pub created_at: Option<String>,

    #[serde(rename = "lastActivityAt", default)]
    pub last_activity_at: Option<String>,

    #[serde(rename = "fileMtime", default)]
    pub file_mtime: Option<u64>,

    #[serde(rename = "gitBranch", default)]
    pub git_branch: Option<String>,
}

/// Represents a message in a JSONL conversation file
#[derive(Debug, Deserialize)]
pub struct ConversationMessage {
    #[serde(rename = "type")]
    pub msg_type: Option<String>,

    #[serde(default)]
    pub role: Option<String>,

    #[serde(default)]
    pub message: Option<MessageContent>,

    #[serde(default)]
    pub timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MessageContent {
    #[serde(default)]
    pub role: Option<String>,

    #[serde(default)]
    pub content: Option<serde_json::Value>,
}

/// Parsed session data ready for indexing
#[derive(Debug)]
pub struct ParsedSession {
    pub session_id: String,
    pub project_path: String,
    pub first_prompt: Option<String>,
    pub summary: Option<String>,
    pub slug: Option<String>,
    pub git_branch: Option<String>,
    pub message_count: usize,
    pub created_at: String,
    pub modified_at: String,
    pub full_text: String,
}

/// Parses a sessions-index.json file into a list of session index entries
pub fn parse_session_index(path: &Path) -> Result<Vec<SessionIndexEntry>> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("Failed to read {:?}", path))?;
    let index: SessionIndex = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse session index {:?}", path))?;
    Ok(index.entries)
}

/// Result of parsing a JSONL conversation file
pub struct ParsedConversation {
    pub full_text: String,
    pub first_prompt: Option<String>,
    pub message_count: usize,
    pub first_timestamp: Option<String>,
    pub last_timestamp: Option<String>,
}

/// Parses a JSONL conversation file and extracts text content
pub fn parse_conversation_jsonl(path: &Path, max_chars: usize) -> Result<ParsedConversation> {
    let file = File::open(path).with_context(|| format!("Failed to open {:?}", path))?;
    let reader = BufReader::new(file);

    let mut full_text = String::new();
    let mut first_prompt: Option<String> = None;
    let mut message_count: usize = 0;
    let mut first_timestamp: Option<String> = None;
    let mut last_timestamp: Option<String> = None;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                log::warn!("Error reading line from {:?}: {}", path, e);
                continue;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let msg: ConversationMessage = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(_) => continue,
        };

        // Track first and last timestamps from all messages
        if let Some(ref ts) = msg.timestamp {
            if first_timestamp.is_none() {
                first_timestamp = Some(ts.clone());
            }
            last_timestamp = Some(ts.clone());
        }

        if let Some(text) = extract_message_text(&msg) {
            if text.trim().is_empty() {
                continue;
            }

            message_count += 1;

            let is_user = is_user_message(&msg);

            // Capture first user prompt
            if is_user && first_prompt.is_none() {
                first_prompt = Some(text.chars().take(500).collect());
            }

            // Skip tool-use noise (messages that look like tool calls/results)
            if is_tool_noise(&text) {
                continue;
            }

            // Add to full text with role prefix for context
            if full_text.len() < max_chars {
                let remaining = max_chars - full_text.len();
                let prefix = if is_user { "User: " } else { "Assistant: " };
                full_text.push_str(prefix);

                if text.len() > remaining {
                    let truncated: String = text.chars().take(remaining).collect();
                    full_text.push_str(&truncated);
                } else {
                    full_text.push_str(&text);
                }
                full_text.push('\n');
            }
        }
    }

    Ok(ParsedConversation {
        full_text,
        first_prompt,
        message_count,
        first_timestamp,
        last_timestamp,
    })
}

/// Extracts text content from a conversation message
fn extract_message_text(msg: &ConversationMessage) -> Option<String> {
    if let Some(ref message) = msg.message {
        if let Some(ref content) = message.content {
            return extract_text_from_content(content);
        }
    }
    None
}

/// Extracts plain text from message content (handles string and array formats)
fn extract_text_from_content(content: &serde_json::Value) -> Option<String> {
    match content {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(arr) => {
            let mut texts = Vec::new();
            for item in arr {
                if let Some(obj) = item.as_object() {
                    // Handle {"type": "text", "text": "..."} format
                    if obj.get("type").and_then(|t| t.as_str()) == Some("text") {
                        if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
                            texts.push(text.to_string());
                        }
                    }
                }
            }
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n"))
            }
        }
        _ => None,
    }
}

/// Checks if a message is from the user
fn is_user_message(msg: &ConversationMessage) -> bool {
    if let Some(ref role) = msg.role {
        return role == "user";
    }
    if let Some(ref message) = msg.message {
        if let Some(ref role) = message.role {
            return role == "user";
        }
    }
    if let Some(ref msg_type) = msg.msg_type {
        return msg_type == "human" || msg_type == "user";
    }
    false
}

/// Detects tool-use noise that shouldn't be indexed
fn is_tool_noise(text: &str) -> bool {
    let trimmed = text.trim();

    // Skip very short messages (likely tool status)
    if trimmed.len() < 5 {
        return true;
    }

    // Skip messages that look like tool calls/results
    if trimmed.starts_with("{\"tool") || trimmed.starts_with("{\"type\":\"tool") {
        return true;
    }

    // Skip file content dumps (very long lines with no spaces)
    if trimmed.len() > 1000 && !trimmed.contains(' ') {
        return true;
    }

    false
}

/// Gets the file modification time as a unix timestamp
pub fn file_mtime(path: &Path) -> Result<i64> {
    let metadata = std::fs::metadata(path).with_context(|| format!("Failed to stat {:?}", path))?;
    let modified = metadata
        .modified()
        .with_context(|| format!("Failed to get mtime for {:?}", path))?;
    let duration = modified
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Ok(duration.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text_from_string_content() {
        let content = serde_json::json!("Hello, world!");
        assert_eq!(
            extract_text_from_content(&content),
            Some("Hello, world!".to_string())
        );
    }

    #[test]
    fn test_extract_text_from_array_content() {
        let content = serde_json::json!([
            {"type": "text", "text": "Hello"},
            {"type": "tool_use", "name": "bash"},
            {"type": "text", "text": "World"}
        ]);
        assert_eq!(
            extract_text_from_content(&content),
            Some("Hello\nWorld".to_string())
        );
    }

    #[test]
    fn test_is_tool_noise() {
        assert!(is_tool_noise("{\"tool_use\": true}"));
        assert!(is_tool_noise("ok"));
        assert!(!is_tool_noise(
            "Please help me fix this bug in the authentication system"
        ));
    }
}
