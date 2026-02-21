pub mod queries;
pub mod schema;

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

use crate::indexer::parser::ParsedSession;

/// Main database handle wrapping rusqlite connection
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Opens or creates the database at the given path
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {:?}", parent))?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open database at {:?}", path))?;

        // Enable WAL mode for better concurrent access
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

        // Create schema (sessions + FTS5 + embeddings)
        schema::create_schema(&conn)?;
        schema::create_vec_table(&conn)?;

        Ok(Self { conn })
    }

    /// Opens an in-memory database (for testing)
    #[allow(dead_code)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        schema::create_schema(&conn)?;
        schema::create_vec_table(&conn)?;

        Ok(Self { conn })
    }

    /// Returns whether vector search is available (always true now — embeddings stored as blobs)
    pub fn has_vector_search(&self) -> bool {
        true
    }

    /// Gets a reference to the underlying connection
    #[allow(dead_code)]
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    // Delegated query methods

    pub fn upsert_session(
        &self,
        session: &ParsedSession,
        file_mtime: i64,
        indexed_at: &str,
    ) -> Result<()> {
        queries::upsert_session(&self.conn, session, file_mtime, indexed_at)
    }

    pub fn upsert_embedding(&self, session_id: &str, embedding: &[f32]) -> Result<()> {
        queries::upsert_embedding(&self.conn, session_id, embedding)
    }

    pub fn get_session_mtime(&self, session_id: &str) -> Result<Option<i64>> {
        queries::get_session_mtime(&self.conn, session_id)
    }

    pub fn fts_search(&self, query: &str, limit: usize) -> Result<Vec<queries::FtsResult>> {
        queries::fts_search(&self.conn, query, limit)
    }

    pub fn vec_search(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<queries::VecResult>> {
        queries::vec_search(&self.conn, query_embedding, limit)
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<queries::SessionRow>> {
        queries::get_session(&self.conn, session_id)
    }

    pub fn list_sessions(
        &self,
        days: Option<u32>,
        project: Option<&str>,
        limit: usize,
    ) -> Result<Vec<queries::SessionRow>> {
        queries::list_sessions(&self.conn, days, project, limit)
    }
}
