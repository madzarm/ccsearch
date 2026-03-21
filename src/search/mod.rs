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
    /// The best matching chunk text for this session (if chunk-based search was used)
    pub matched_text: Option<String>,
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
    exclude_projects: &[String],
    exact: bool,
) -> Result<Vec<SearchResult>> {
    // BM25 search (uses chunks if available, falls back to sessions)
    let bm25_results = bm25::search(db, query, limit * 2, exact)?;

    // Vector search (uses chunk embeddings if available, falls back to session embeddings)
    let vec_results = if let Some(embedder) = embedder {
        vector::search(db, embedder, query, limit * 2)?
    } else {
        Vec::new()
    };

    // RRF fusion
    let fused = rrf::fuse(&bm25_results, &vec_results, bm25_weight, vec_weight, rrf_k);

    let now = chrono::Utc::now();

    // Build FTS5 query for chunk text retrieval
    let fts_query = bm25::build_fts5_query(query);

    // Fetch full session data and apply recency boost
    let mut results = Vec::new();
    for rrf_result in fused.into_iter().take(limit * 2) {
        if let Ok(Some(session)) = db.get_session(&rrf_result.session_id) {
            // Skip excluded projects
            if exclude_projects.iter().any(|ex| {
                session
                    .project_path
                    .to_lowercase()
                    .contains(&ex.to_lowercase())
            }) {
                continue;
            }

            let score = if recency_halflife > 0.0 {
                let age_days = chrono::DateTime::parse_from_rfc3339(&session.modified_at)
                    .map(|dt| (now - dt.to_utc()).num_hours() as f64 / 24.0)
                    .unwrap_or(recency_halflife);
                let boost = 1.0 + (0.5f64.powf(age_days / recency_halflife));
                rrf_result.score * boost
            } else {
                rrf_result.score
            };

            // Get the best matching chunk text for preview
            let matched_text = if !fts_query.is_empty() {
                db.get_best_matching_chunk(&fts_query, &rrf_result.session_id)
                    .unwrap_or(None)
            } else {
                None
            };

            results.push(SearchResult {
                session_id: rrf_result.session_id,
                score,
                bm25_rank: rrf_result.bm25_rank,
                vec_rank: rrf_result.vec_rank,
                session,
                matched_text,
            });
        }
    }

    // Re-sort after recency boost
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);

    Ok(results)
}
