use std::collections::HashMap;

use crate::db::queries::{FtsResult, VecResult};

/// A search result after Reciprocal Rank Fusion
#[derive(Debug, Clone)]
pub struct RrfResult {
    pub session_id: String,
    pub score: f64,
    pub bm25_rank: Option<usize>,
    pub vec_rank: Option<usize>,
}

/// Combines BM25 and vector search results using Reciprocal Rank Fusion.
///
/// RRF formula: score = Σ (weight_i / (rank_i + k))
///
/// - `k` is a constant (default 60) that controls how much weight is given to lower-ranked items
/// - Higher k = more equal weighting across ranks
/// - Lower k = more weight to top-ranked items
pub fn fuse(
    bm25_results: &[FtsResult],
    vec_results: &[VecResult],
    bm25_weight: f64,
    vec_weight: f64,
    k: f64,
) -> Vec<RrfResult> {
    let mut scores: HashMap<String, RrfResult> = HashMap::new();

    // Add BM25 scores
    for (rank, result) in bm25_results.iter().enumerate() {
        let entry = scores
            .entry(result.session_id.clone())
            .or_insert_with(|| RrfResult {
                session_id: result.session_id.clone(),
                score: 0.0,
                bm25_rank: None,
                vec_rank: None,
            });
        entry.score += bm25_weight / (rank as f64 + 1.0 + k);
        entry.bm25_rank = Some(rank + 1);
    }

    // Add vector scores
    for (rank, result) in vec_results.iter().enumerate() {
        let entry = scores
            .entry(result.session_id.clone())
            .or_insert_with(|| RrfResult {
                session_id: result.session_id.clone(),
                score: 0.0,
                bm25_rank: None,
                vec_rank: None,
            });
        entry.score += vec_weight / (rank as f64 + 1.0 + k);
        entry.vec_rank = Some(rank + 1);
    }

    // Sort by score descending
    let mut results: Vec<RrfResult> = scores.into_values().collect();
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_basic() {
        let bm25 = vec![
            FtsResult {
                session_id: "a".into(),
                rank: -5.0,
            },
            FtsResult {
                session_id: "b".into(),
                rank: -3.0,
            },
            FtsResult {
                session_id: "c".into(),
                rank: -1.0,
            },
        ];
        let vec = vec![
            VecResult {
                session_id: "b".into(),
                distance: 0.1,
            },
            VecResult {
                session_id: "d".into(),
                distance: 0.2,
            },
            VecResult {
                session_id: "a".into(),
                distance: 0.3,
            },
        ];

        let results = fuse(&bm25, &vec, 1.0, 1.0, 60.0);

        // "b" should score highest (rank 2 in BM25, rank 1 in vec)
        // or "a" (rank 1 in BM25, rank 3 in vec)
        assert!(!results.is_empty());

        // Both "a" and "b" should appear in results
        let ids: Vec<&str> = results.iter().map(|r| r.session_id.as_str()).collect();
        assert!(ids.contains(&"a"));
        assert!(ids.contains(&"b"));
        assert!(ids.contains(&"c"));
        assert!(ids.contains(&"d"));
    }

    #[test]
    fn test_rrf_empty() {
        let results = fuse(&[], &[], 1.0, 1.0, 60.0);
        assert!(results.is_empty());
    }

    #[test]
    fn test_rrf_bm25_only() {
        let bm25 = vec![
            FtsResult {
                session_id: "a".into(),
                rank: -5.0,
            },
            FtsResult {
                session_id: "b".into(),
                rank: -3.0,
            },
        ];
        let results = fuse(&bm25, &[], 1.0, 1.0, 60.0);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].session_id, "a"); // rank 1 should be highest
    }

    #[test]
    fn test_rrf_weights() {
        let bm25 = vec![FtsResult {
            session_id: "a".into(),
            rank: -5.0,
        }];
        let vec = vec![VecResult {
            session_id: "b".into(),
            distance: 0.1,
        }];

        // With high BM25 weight, "a" should score higher
        let results = fuse(&bm25, &vec, 10.0, 1.0, 60.0);
        assert_eq!(results[0].session_id, "a");

        // With high vec weight, "b" should score higher
        let results = fuse(&bm25, &vec, 1.0, 10.0, 60.0);
        assert_eq!(results[0].session_id, "b");
    }
}
