use anyhow::Result;

use crate::db::queries::FtsResult;
use crate::db::Database;

/// Performs BM25 keyword search using SQLite FTS5
pub fn search(db: &Database, query: &str, limit: usize) -> Result<Vec<FtsResult>> {
    // FTS5 query syntax: we need to escape special characters
    let sanitized = sanitize_fts5_query(query);

    if sanitized.is_empty() {
        return Ok(Vec::new());
    }

    db.fts_search(&sanitized, limit)
}

/// Sanitizes a query string for FTS5 MATCH syntax.
/// Converts natural language queries into valid FTS5 queries.
fn sanitize_fts5_query(query: &str) -> String {
    // Split into words and join with implicit AND
    let words: Vec<&str> = query
        .split(|c: char| !c.is_alphanumeric() && c != '*' && c != '"')
        .filter(|w| !w.is_empty())
        .collect();

    if words.is_empty() {
        return String::new();
    }

    // Join words with OR for broader matching
    // FTS5 uses implicit AND by default, we use OR for better recall
    words
        .iter()
        .map(|w| {
            // Add prefix matching for short words
            if w.len() >= 3 && !w.ends_with('*') && !w.contains('"') {
                format!("\"{}\" OR {}*", w, w)
            } else {
                format!("\"{}\"", w)
            }
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_simple_query() {
        let result = sanitize_fts5_query("authentication bug");
        assert!(result.contains("authentication"));
        assert!(result.contains("bug"));
    }

    #[test]
    fn test_sanitize_empty_query() {
        assert_eq!(sanitize_fts5_query(""), "");
        assert_eq!(sanitize_fts5_query("   "), "");
    }

    #[test]
    fn test_sanitize_special_chars() {
        let result = sanitize_fts5_query("fix: auth-bug (urgent)");
        // Should handle special chars without crashing
        assert!(result.contains("fix"));
        assert!(result.contains("auth"));
    }
}
