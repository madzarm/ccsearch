use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Helper to set up a test database with fixture data
fn setup_test_db() -> ccsearch::db::Database {
    let db = ccsearch::db::Database::open_in_memory().unwrap();

    let index_path = fixture_path("sessions-index.json");
    let entries = ccsearch::indexer::parser::parse_session_index(&index_path).unwrap();

    let session_files = [
        "sample-session.jsonl",
        "sample-session-2.jsonl",
        "sample-session-3.jsonl",
    ];

    for (entry, filename) in entries.iter().zip(session_files.iter()) {
        let jsonl_path = fixture_path(filename);
        let parsed =
            ccsearch::indexer::parser::parse_conversation_jsonl(&jsonl_path, 8000).unwrap();

        let session = ccsearch::indexer::parser::ParsedSession {
            session_id: entry.session_id.clone(),
            project_path: entry
                .project_path
                .clone()
                .unwrap_or_else(|| "/test".to_string()),
            first_prompt: parsed.first_prompt,
            summary: entry.summary.clone(),
            slug: entry.slug.clone(),
            git_branch: entry.git_branch.clone(),
            message_count: parsed.message_count,
            created_at: entry
                .created_at
                .clone()
                .unwrap_or_else(|| "2026-02-15T10:00:00Z".to_string()),
            modified_at: entry
                .last_activity_at
                .clone()
                .unwrap_or_else(|| "2026-02-15T10:00:00Z".to_string()),
            full_text: parsed.full_text,
        };

        let now = chrono::Utc::now().to_rfc3339();
        db.upsert_session(&session, 0, &now).unwrap();
    }

    db
}

#[test]
fn test_fts_search_auth() {
    let db = setup_test_db();

    let results = db.fts_search("authentication", 10).unwrap();
    assert!(
        !results.is_empty(),
        "Should find results for 'authentication'"
    );
    assert_eq!(
        results[0].session_id, "abc12345-1111-2222-3333-444455556666",
        "Auth session should be top result"
    );
}

#[test]
fn test_fts_search_dark_mode() {
    let db = setup_test_db();

    let results = db.fts_search("\"dark mode\"", 10).unwrap();
    assert!(!results.is_empty(), "Should find results for 'dark mode'");
    assert_eq!(
        results[0].session_id, "def67890-aaaa-bbbb-cccc-ddddeeeeffff",
        "Settings session should be top result for dark mode"
    );
}

#[test]
fn test_fts_search_database() {
    let db = setup_test_db();

    let results = db.fts_search("database", 10).unwrap();
    assert!(!results.is_empty(), "Should find results for 'database'");

    // The DB refactor session should appear in results
    let ids: Vec<&str> = results.iter().map(|r| r.session_id.as_str()).collect();
    assert!(
        ids.contains(&"ghi11111-2222-3333-4444-555566667777"),
        "DB refactor session should be in results"
    );
}

#[test]
fn test_fts_search_no_results() {
    let db = setup_test_db();

    let results = db.fts_search("\"zzzznonexistent1234\"", 10).unwrap();
    assert!(results.is_empty(), "Should return no results for gibberish");
}

#[test]
fn test_list_sessions() {
    let db = setup_test_db();

    let sessions = db.list_sessions(None, None, 100).unwrap();
    assert_eq!(sessions.len(), 3, "Should have 3 sessions");
}

#[test]
fn test_list_sessions_with_project_filter() {
    let db = setup_test_db();

    let sessions = db.list_sessions(None, Some("webapp"), 100).unwrap();
    assert_eq!(sessions.len(), 3, "All sessions are from webapp project");

    let sessions = db.list_sessions(None, Some("nonexistent"), 100).unwrap();
    assert!(sessions.is_empty(), "No sessions for nonexistent project");
}

#[test]
fn test_get_session() {
    let db = setup_test_db();

    let session = db
        .get_session("abc12345-1111-2222-3333-444455556666")
        .unwrap();
    assert!(session.is_some());

    let session = session.unwrap();
    assert_eq!(
        session.summary.as_deref(),
        Some("Fix authentication bug in login flow")
    );
    assert_eq!(session.git_branch.as_deref(), Some("fix/auth-bug"));
}

#[test]
fn test_upsert_replaces_existing() {
    let db = setup_test_db();

    // Get original
    let orig = db
        .get_session("abc12345-1111-2222-3333-444455556666")
        .unwrap()
        .unwrap();
    assert_eq!(
        orig.summary.as_deref(),
        Some("Fix authentication bug in login flow")
    );

    // Upsert with new data
    let session = ccsearch::indexer::parser::ParsedSession {
        session_id: "abc12345-1111-2222-3333-444455556666".to_string(),
        project_path: "/test".to_string(),
        first_prompt: Some("Updated prompt".to_string()),
        summary: Some("Updated summary".to_string()),
        slug: None,
        git_branch: None,
        message_count: 1,
        created_at: "2026-02-15T10:00:00Z".to_string(),
        modified_at: "2026-02-15T10:00:00Z".to_string(),
        full_text: "Updated text".to_string(),
    };

    let now = chrono::Utc::now().to_rfc3339();
    db.upsert_session(&session, 999, &now).unwrap();

    // Verify updated
    let updated = db
        .get_session("abc12345-1111-2222-3333-444455556666")
        .unwrap()
        .unwrap();
    assert_eq!(updated.summary.as_deref(), Some("Updated summary"));

    // FTS should also be updated
    let results = db.fts_search("\"Updated summary\"", 10).unwrap();
    assert!(!results.is_empty());
}
