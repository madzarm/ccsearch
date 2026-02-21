pub mod queries;
pub mod schema;

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

use crate::indexer::parser::ParsedSession;

/// Main database handle wrapping rusqlite connection
pub struct Database {
    conn: Connection,
    has_vec: bool,
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

        // Create base schema (sessions + FTS5)
        schema::create_schema(&conn)?;

        // Try to load sqlite-vec extension
        let has_vec = Self::try_load_sqlite_vec(&conn);

        if has_vec {
            schema::create_vec_table(&conn)?;
            log::debug!("sqlite-vec extension loaded successfully");
        } else {
            log::info!("sqlite-vec not available, vector search disabled");
        }

        Ok(Self { conn, has_vec })
    }

    /// Opens an in-memory database (for testing)
    #[allow(dead_code)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        schema::create_schema(&conn)?;

        let has_vec = Self::try_load_sqlite_vec(&conn);
        if has_vec {
            schema::create_vec_table(&conn)?;
        }

        Ok(Self { conn, has_vec })
    }

    /// Attempts to load the sqlite-vec extension
    fn try_load_sqlite_vec(conn: &Connection) -> bool {
        // Test if vec0 is already available (e.g., compiled into SQLite)
        let test = conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS _vec_test USING vec0(test_col float[2]);
             DROP TABLE IF EXISTS _vec_test;",
        );
        test.is_ok()
    }

    /// Returns whether vector search is available
    pub fn has_vector_search(&self) -> bool {
        self.has_vec
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
        if !self.has_vec {
            return Ok(()); // Silently skip if vec not available
        }
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
        if !self.has_vec {
            return Ok(Vec::new());
        }
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
