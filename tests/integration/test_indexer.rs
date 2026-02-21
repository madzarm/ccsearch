use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn test_parse_session_index() {
    let path = fixture_path("sessions-index.json");
    let entries = ccsearch::indexer::parser::parse_session_index(&path).unwrap();

    assert_eq!(entries.len(), 3);
    assert_eq!(
        entries[0].session_id,
        "abc12345-1111-2222-3333-444455556666"
    );
    assert_eq!(
        entries[0].summary.as_deref(),
        Some("Fix authentication bug in login flow")
    );
    assert_eq!(entries[0].git_branch.as_deref(), Some("fix/auth-bug"));
}

#[test]
fn test_parse_conversation_jsonl() {
    let path = fixture_path("sample-session.jsonl");
    let (full_text, first_prompt, message_count) =
        ccsearch::indexer::parser::parse_conversation_jsonl(&path, 8000).unwrap();

    // Should have extracted text
    assert!(!full_text.is_empty());

    // First prompt should be the user's first message
    assert!(first_prompt.is_some());
    let prompt = first_prompt.unwrap();
    assert!(prompt.contains("401 error"));

    // Should have counted messages
    assert!(message_count > 0);

    // Full text should contain conversation content
    assert!(full_text.contains("authentication"));
    assert!(full_text.contains("refresh token"));
}

#[test]
fn test_parse_conversation_truncation() {
    let path = fixture_path("sample-session.jsonl");
    // Very small max_chars to test truncation
    let (full_text, _, _) =
        ccsearch::indexer::parser::parse_conversation_jsonl(&path, 100).unwrap();

    assert!(full_text.len() <= 200); // Some overhead from prefixes
}

#[test]
fn test_file_mtime() {
    let path = fixture_path("sessions-index.json");
    let mtime = ccsearch::indexer::parser::file_mtime(&path).unwrap();

    // mtime should be a reasonable Unix timestamp (after 2020)
    assert!(mtime > 1577836800); // 2020-01-01
}
