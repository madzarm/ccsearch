use anyhow::Result;

use crate::db::queries::FtsResult;
use crate::db::Database;

/// Performs BM25 keyword search using SQLite FTS5
pub fn search(db: &Database, query: &str, limit: usize, exact: bool) -> Result<Vec<FtsResult>> {
    let sanitized = if exact {
        build_fts5_phrase(query)
    } else {
        build_fts5_query(query)
    };

    if sanitized.is_empty() {
        return Ok(Vec::new());
    }

    db.fts_search(&sanitized, limit)
}

/// Builds a valid FTS5 MATCH query from a natural language query string.
/// Splits into words, each gets exact+prefix matching, AND-ed together.
pub fn build_fts5_query(query: &str) -> String {
    let words: Vec<&str> = query
        .split(|c: char| !c.is_alphanumeric() && c != '*' && c != '"')
        .filter(|w| !w.is_empty())
        .collect();

    if words.is_empty() {
        return String::new();
    }

    words
        .iter()
        .map(|w| {
            if w.len() >= 3 && !w.ends_with('*') && !w.contains('"') {
                format!("(\"{}\" OR {}*)", w, w)
            } else {
                format!("\"{}\"", w)
            }
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// Builds an exact phrase FTS5 query — matches the literal token sequence.
fn build_fts5_phrase(query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    format!("\"{}\"", trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_simple() {
        let result = build_fts5_query("authentication bug");
        assert!(result.contains("authentication"));
        assert!(result.contains("bug"));
        assert!(result.contains("AND"));
    }

    #[test]
    fn test_query_empty() {
        assert_eq!(build_fts5_query(""), "");
        assert_eq!(build_fts5_query("   "), "");
    }

    #[test]
    fn test_query_special_chars() {
        let result = build_fts5_query("fix: auth-bug (urgent)");
        assert!(result.contains("fix"));
        assert!(result.contains("auth"));
    }

    #[test]
    fn test_phrase_exact() {
        assert_eq!(build_fts5_phrase("phase 1"), "\"phase 1\"");
        assert_eq!(build_fts5_phrase(""), "");
    }
}
