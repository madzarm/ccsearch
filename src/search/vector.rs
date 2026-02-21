use anyhow::Result;

use crate::db::queries::VecResult;
use crate::db::Database;
use crate::indexer::embedder::Embedder;

/// Performs vector similarity search using sqlite-vec
pub fn search(
    db: &Database,
    embedder: &mut Embedder,
    query: &str,
    limit: usize,
) -> Result<Vec<VecResult>> {
    if !db.has_vector_search() {
        return Ok(Vec::new());
    }

    let query_embedding = embedder.embed(query)?;
    db.vec_search(&query_embedding, limit)
}
