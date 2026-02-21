pub mod bm25;
pub mod rrf;
pub mod vector;

use anyhow::Result;

use crate::db::queries::SessionRow;
use crate::db::Database;
use crate::indexer::embedder::Embedder;

/// A ranked search result with metadata
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub session_id: String,
    pub score: f64,
    pub bm25_rank: Option<usize>,
    pub vec_rank: Option<usize>,
    pub session: SessionRow,
}

/// Performs hybrid search: BM25 + vector + RRF fusion
pub fn hybrid_search(
    db: &Database,
    embedder: Option<&mut Embedder>,
    query: &str,
    limit: usize,
    bm25_weight: f64,
    vec_weight: f64,
    rrf_k: f64,
) -> Result<Vec<SearchResult>> {
    // BM25 search
    let bm25_results = bm25::search(db, query, limit * 2)?;

    // Vector search (if embedder available)
    let vec_results = if let Some(embedder) = embedder {
        vector::search(db, embedder, query, limit * 2)?
    } else {
        Vec::new()
    };

    // RRF fusion
    let fused = rrf::fuse(&bm25_results, &vec_results, bm25_weight, vec_weight, rrf_k);

    // Fetch full session data for top results
    let mut results = Vec::new();
    for rrf_result in fused.into_iter().take(limit) {
        if let Ok(Some(session)) = db.get_session(&rrf_result.session_id) {
            results.push(SearchResult {
                session_id: rrf_result.session_id,
                score: rrf_result.score,
                bm25_rank: rrf_result.bm25_rank,
                vec_rank: rrf_result.vec_rank,
                session,
            });
        }
    }

    Ok(results)
}
