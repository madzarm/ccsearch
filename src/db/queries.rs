use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::indexer::parser::ParsedSession;

/// Search result from BM25 (FTS5) query
#[derive(Debug, Clone)]
pub struct FtsResult {
    pub session_id: String,
    #[allow(dead_code)]
    pub rank: f64,
}

/// Search result from vector similarity query
#[derive(Debug, Clone)]
pub struct VecResult {
    pub session_id: String,
    #[allow(dead_code)]
    pub distance: f64,
}

/// Full session row from the database
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionRow {
    pub session_id: String,
    pub project_path: String,
    pub first_prompt: Option<String>,
    pub summary: Option<String>,
    pub slug: Option<String>,
    pub git_branch: Option<String>,
    pub message_count: Option<i64>,
    pub created_at: String,
    pub modified_at: String,
    pub full_text: String,
}

/// Upserts a session into the sessions table
pub fn upsert_session(
    conn: &Connection,
    session: &ParsedSession,
    file_mtime: i64,
    indexed_at: &str,
) -> Result<()> {
    // Delete first to trigger FTS cleanup, then insert
    conn.execute(
        "DELETE FROM sessions WHERE session_id = ?1",
        params![session.session_id],
    )?;

    conn.execute(
        "INSERT INTO sessions (
            session_id, project_path, first_prompt, summary, slug,
            git_branch, message_count, created_at, modified_at,
            file_mtime, indexed_at, full_text
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            session.session_id,
            session.project_path,
            session.first_prompt,
            session.summary,
            session.slug,
            session.git_branch,
            session.message_count as i64,
            session.created_at,
            session.modified_at,
            file_mtime,
            indexed_at,
            session.full_text,
        ],
    )
    .context("Failed to insert session")?;

    Ok(())
}

/// Upserts a vector embedding for a session
pub fn upsert_embedding(conn: &Connection, session_id: &str, embedding: &[f32]) -> Result<()> {
    let bytes = embedding_to_bytes(embedding);
    conn.execute(
        "INSERT OR REPLACE INTO session_embeddings (session_id, embedding) VALUES (?1, ?2)",
        params![session_id, bytes],
    )
    .context("Failed to insert embedding")?;

    Ok(())
}

/// Gets the stored file_mtime for a session (for staleness detection)
pub fn get_session_mtime(conn: &Connection, session_id: &str) -> Result<Option<i64>> {
    let mut stmt = conn.prepare("SELECT file_mtime FROM sessions WHERE session_id = ?1")?;
    let result = stmt
        .query_row(params![session_id], |row| row.get(0))
        .optional()?;
    Ok(result)
}

/// BM25 full-text search using FTS5
pub fn fts_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<FtsResult>> {
    let mut stmt = conn.prepare(
        "SELECT session_id, rank
         FROM sessions_fts
         WHERE sessions_fts MATCH ?1
         ORDER BY rank
         LIMIT ?2",
    )?;

    let rows = stmt.query_map(params![query, limit as i64], |row| {
        Ok(FtsResult {
            session_id: row.get(0)?,
            rank: row.get(1)?,
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        match row {
            Ok(r) => results.push(r),
            Err(e) => log::warn!("FTS query row error: {}", e),
        }
    }

    Ok(results)
}

/// Vector similarity search — loads all embeddings and computes cosine similarity in Rust
pub fn vec_search(
    conn: &Connection,
    query_embedding: &[f32],
    limit: usize,
) -> Result<Vec<VecResult>> {
    let mut stmt = conn.prepare("SELECT session_id, embedding FROM session_embeddings")?;

    let rows = stmt.query_map([], |row| {
        let session_id: String = row.get(0)?;
        let blob: Vec<u8> = row.get(1)?;
        Ok((session_id, blob))
    })?;

    let mut scored: Vec<(String, f64)> = Vec::new();
    for row in rows {
        match row {
            Ok((session_id, blob)) => {
                let embedding = bytes_to_embedding(&blob);
                let sim = cosine_similarity(query_embedding, &embedding);
                scored.push((session_id, sim));
            }
            Err(e) => log::warn!("Vec query row error: {}", e),
        }
    }

    // Sort by similarity descending (highest = most similar)
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);

    Ok(scored
        .into_iter()
        .map(|(session_id, sim)| VecResult {
            session_id,
            distance: 1.0 - sim, // convert similarity to distance for consistency
        })
        .collect())
}

/// Gets a full session row by ID
pub fn get_session(conn: &Connection, session_id: &str) -> Result<Option<SessionRow>> {
    let mut stmt = conn.prepare(
        "SELECT session_id, project_path, first_prompt, summary, slug,
                git_branch, message_count, created_at, modified_at, full_text
         FROM sessions
         WHERE session_id = ?1",
    )?;

    let result = stmt
        .query_row(params![session_id], |row| {
            Ok(SessionRow {
                session_id: row.get(0)?,
                project_path: row.get(1)?,
                first_prompt: row.get(2)?,
                summary: row.get(3)?,
                slug: row.get(4)?,
                git_branch: row.get(5)?,
                message_count: row.get(6)?,
                created_at: row.get(7)?,
                modified_at: row.get(8)?,
                full_text: row.get(9)?,
            })
        })
        .optional()?;

    Ok(result)
}

/// Lists sessions with optional filtering
pub fn list_sessions(
    conn: &Connection,
    days: Option<u32>,
    project: Option<&str>,
    limit: usize,
) -> Result<Vec<SessionRow>> {
    let mut sql = String::from(
        "SELECT session_id, project_path, first_prompt, summary, slug,
                git_branch, message_count, created_at, modified_at, full_text
         FROM sessions WHERE 1=1",
    );

    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(days) = days {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
        sql.push_str(&format!(" AND created_at >= ?{}", param_idx));
        param_values.push(Box::new(cutoff.to_rfc3339()));
        param_idx += 1;
    }

    if let Some(project) = project {
        sql.push_str(&format!(" AND project_path LIKE ?{}", param_idx));
        param_values.push(Box::new(format!("%{}%", project)));
        param_idx += 1;
    }

    sql.push_str(&format!(" ORDER BY modified_at DESC LIMIT ?{}", param_idx));
    param_values.push(Box::new(limit as i64));

    let mut stmt = conn.prepare(&sql)?;
    let params_refs: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        Ok(SessionRow {
            session_id: row.get(0)?,
            project_path: row.get(1)?,
            first_prompt: row.get(2)?,
            summary: row.get(3)?,
            slug: row.get(4)?,
            git_branch: row.get(5)?,
            message_count: row.get(6)?,
            created_at: row.get(7)?,
            modified_at: row.get(8)?,
            full_text: row.get(9)?,
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }

    Ok(results)
}

/// Converts f32 slice to little-endian bytes for storage
fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Converts little-endian bytes back to f32 slice
fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

/// Computes cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        let x = *x as f64;
        let y = *y as f64;
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// Trait extension for optional query results
trait OptionalResult<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalResult<T> for std::result::Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
