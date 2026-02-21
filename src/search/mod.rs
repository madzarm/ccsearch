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

/// Performs hybrid search: BM25 + vector + RRF fusion + recency boost
pub fn hybrid_search(
    db: &Database,
    embedder: Option<&mut Embedder>,
    query: &str,
    limit: usize,
    bm25_weight: f64,
    vec_weight: f64,
    rrf_k: f64,
    recency_halflife: f64,
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

    let now = chrono::Utc::now();

    // Fetch full session data and apply recency boost
    let mut results = Vec::new();
    for rrf_result in fused.into_iter().take(limit * 2) {
        if let Ok(Some(session)) = db.get_session(&rrf_result.session_id) {
            let score = if recency_halflife > 0.0 {
                let age_days = chrono::DateTime::parse_from_rfc3339(&session.modified_at)
                    .map(|dt| (now - dt.to_utc()).num_hours() as f64 / 24.0)
                    .unwrap_or(recency_halflife);
                let boost = 1.0 + (0.5f64.powf(age_days / recency_halflife));
                rrf_result.score * boost
            } else {
                rrf_result.score
            };

            results.push(SearchResult {
                session_id: rrf_result.session_id,
                score,
                bm25_rank: rrf_result.bm25_rank,
                vec_rank: rrf_result.vec_rank,
                session,
            });
        }
    }

    // Re-sort after recency boost
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);

    Ok(results)
}
