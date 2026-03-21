use anyhow::Result;
use rusqlite::Connection;

/// Creates all tables and triggers for the ccsearch database
pub fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        -- Session metadata
        CREATE TABLE IF NOT EXISTS sessions (
            session_id TEXT PRIMARY KEY,
            project_path TEXT NOT NULL,
            first_prompt TEXT,
            summary TEXT,
            slug TEXT,
            git_branch TEXT,
            message_count INTEGER,
            created_at TEXT NOT NULL,
            modified_at TEXT NOT NULL,
            file_mtime INTEGER NOT NULL,
            indexed_at TEXT NOT NULL,
            full_text TEXT NOT NULL DEFAULT ''
        );

        -- FTS5 virtual table for BM25 keyword search
        CREATE VIRTUAL TABLE IF NOT EXISTS sessions_fts USING fts5(
            session_id UNINDEXED,
            first_prompt,
            summary,
            full_text,
            content='sessions',
            content_rowid='rowid'
        );

        -- Index metadata for staleness tracking
        CREATE TABLE IF NOT EXISTS index_meta (
            key TEXT PRIMARY KEY,
            value TEXT
        );

        -- Conversation chunks for fine-grained search
        CREATE TABLE IF NOT EXISTS chunks (
            chunk_id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            chunk_index INTEGER NOT NULL,
            text TEXT NOT NULL DEFAULT '',
            UNIQUE(session_id, chunk_index)
        );

        -- FTS5 on chunks for BM25 keyword search
        -- Column mapping (positional, excluding content_rowid):
        --   FTS5[0] session_id  -> chunks.session_id
        --   FTS5[1] chunk_index -> chunks.chunk_index
        --   FTS5[2] text        -> chunks.text
        CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
            session_id UNINDEXED,
            chunk_index UNINDEXED,
            text,
            content='chunks',
            content_rowid='chunk_id'
        );
        ",
    )?;

    // Create triggers (use IF NOT EXISTS workaround: drop and recreate)
    conn.execute_batch(
        "
        DROP TRIGGER IF EXISTS sessions_ai;
        CREATE TRIGGER sessions_ai AFTER INSERT ON sessions BEGIN
            INSERT INTO sessions_fts(rowid, session_id, first_prompt, summary, full_text)
            VALUES (new.rowid, new.session_id, new.first_prompt, new.summary, new.full_text);
        END;

        DROP TRIGGER IF EXISTS sessions_ad;
        CREATE TRIGGER sessions_ad AFTER DELETE ON sessions BEGIN
            INSERT INTO sessions_fts(sessions_fts, rowid, session_id, first_prompt, summary, full_text)
            VALUES ('delete', old.rowid, old.session_id, old.first_prompt, old.summary, old.full_text);
        END;

        DROP TRIGGER IF EXISTS sessions_au;
        CREATE TRIGGER sessions_au AFTER UPDATE ON sessions BEGIN
            INSERT INTO sessions_fts(sessions_fts, rowid, session_id, first_prompt, summary, full_text)
            VALUES ('delete', old.rowid, old.session_id, old.first_prompt, old.summary, old.full_text);
            INSERT INTO sessions_fts(rowid, session_id, first_prompt, summary, full_text)
            VALUES (new.rowid, new.session_id, new.first_prompt, new.summary, new.full_text);
        END;

        DROP TRIGGER IF EXISTS chunks_ai;
        CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
            INSERT INTO chunks_fts(rowid, session_id, chunk_index, text)
            VALUES (new.chunk_id, new.session_id, new.chunk_index, new.text);
        END;

        DROP TRIGGER IF EXISTS chunks_ad;
        CREATE TRIGGER chunks_ad AFTER DELETE ON chunks BEGIN
            INSERT INTO chunks_fts(chunks_fts, rowid, session_id, chunk_index, text)
            VALUES ('delete', old.chunk_id, old.session_id, old.chunk_index, old.text);
        END;

        DROP TRIGGER IF EXISTS chunks_au;
        CREATE TRIGGER chunks_au AFTER UPDATE ON chunks BEGIN
            INSERT INTO chunks_fts(chunks_fts, rowid, session_id, chunk_index, text)
            VALUES ('delete', old.chunk_id, old.session_id, old.chunk_index, old.text);
            INSERT INTO chunks_fts(rowid, session_id, chunk_index, text)
            VALUES (new.chunk_id, new.session_id, new.chunk_index, new.text);
        END;
        ",
    )?;

    Ok(())
}

/// Creates the vector embedding tables (plain tables with blob storage)
pub fn create_vec_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS session_embeddings (
            session_id TEXT PRIMARY KEY,
            embedding BLOB NOT NULL
        );

        CREATE TABLE IF NOT EXISTS chunk_embeddings (
            chunk_id INTEGER PRIMARY KEY,
            session_id TEXT NOT NULL,
            embedding BLOB NOT NULL
        );
        ",
    )?;
    Ok(())
}
