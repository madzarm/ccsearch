use anyhow::Result;

use crate::db::queries::FtsResult;
use crate::db::Database;

/// Performs BM25 keyword search using SQLite FTS5
pub fn search(db: &Database, query: &str, limit: usize) -> Result<Vec<FtsResult>> {
    let sanitized = build_fts5_query(query);

    if sanitized.is_empty() {
        return Ok(Vec::new());
    }

    db.fts_search(&sanitized, limit)
}

/// Builds a valid FTS5 MATCH query from a natural language query string.
pub fn build_fts5_query(query: &str) -> String {
    sanitize_fts5_query(query)
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

    // Each word becomes (exact OR prefix*) to catch inflections,
    // then words are AND-ed together so all terms must be present.
    // FTS5: adjacent expressions without an operator use implicit AND.
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
        .join(" ")
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
