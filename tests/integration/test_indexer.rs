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
    let parsed =
        ccsearch::indexer::parser::parse_conversation_jsonl(&path).unwrap();

    // Should have extracted text
    assert!(!parsed.full_text.is_empty());

    // First prompt should be the user's first message
    assert!(parsed.first_prompt.is_some());
    let prompt = parsed.first_prompt.unwrap();
    assert!(prompt.contains("401 error"));

    // Should have counted messages
    assert!(parsed.message_count > 0);

    // Full text should contain conversation content
    assert!(parsed.full_text.contains("authentication"));
    assert!(parsed.full_text.contains("refresh token"));
}

#[test]
fn test_chunk_text() {
    let text = "a".repeat(10000);
    let chunks = ccsearch::indexer::parser::chunk_text(&text, 4000, 200);

    // Should produce multiple chunks
    assert!(chunks.len() > 1, "Long text should produce multiple chunks");

    // Each chunk should be at most chunk_size
    for chunk in &chunks {
        assert!(chunk.len() <= 4000);
    }

    // Short text should produce one chunk
    let short = "hello world";
    let chunks = ccsearch::indexer::parser::chunk_text(short, 4000, 200);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0], short);

    // Empty text should produce no chunks
    let chunks = ccsearch::indexer::parser::chunk_text("", 4000, 200);
    assert!(chunks.is_empty());
}

#[test]
fn test_file_mtime() {
    let path = fixture_path("sessions-index.json");
    let mtime = ccsearch::indexer::parser::file_mtime(&path).unwrap();

    // mtime should be a reasonable Unix timestamp (after 2020)
    assert!(mtime > 1577836800); // 2020-01-01
}
