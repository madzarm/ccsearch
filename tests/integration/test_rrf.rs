use ccsearch::db::queries::{FtsResult, VecResult};
use ccsearch::search::rrf;

#[test]
fn test_rrf_known_answer() {
    // Known-answer test with predictable ranking
    let bm25 = vec![
        FtsResult {
            session_id: "s1".into(),
            rank: -10.0,
        },
        FtsResult {
            session_id: "s2".into(),
            rank: -8.0,
        },
        FtsResult {
            session_id: "s3".into(),
            rank: -5.0,
        },
    ];

    let vec = vec![
        VecResult {
            session_id: "s2".into(),
            distance: 0.1,
        },
        VecResult {
            session_id: "s4".into(),
            distance: 0.2,
        },
        VecResult {
            session_id: "s1".into(),
            distance: 0.3,
        },
    ];

    let results = rrf::fuse(&bm25, &vec, 1.0, 1.0, 60.0);

    // s1: BM25 rank 1 -> 1/(1+60) = 0.01639, Vec rank 3 -> 1/(3+60) = 0.01587 => total ~0.03226
    // s2: BM25 rank 2 -> 1/(2+60) = 0.01613, Vec rank 1 -> 1/(1+60) = 0.01639 => total ~0.03252
    // s3: BM25 rank 3 -> 1/(3+60) = 0.01587, Vec absent => total ~0.01587
    // s4: BM25 absent, Vec rank 2 -> 1/(2+60) = 0.01613 => total ~0.01613

    // s2 should be first (highest combined score)
    assert_eq!(results[0].session_id, "s2");

    // s1 should be second
    assert_eq!(results[1].session_id, "s1");

    // Both s2 and s1 should have both ranks
    assert!(results[0].bm25_rank.is_some());
    assert!(results[0].vec_rank.is_some());

    // s3 and s4 should have only one rank each
    let s3 = results.iter().find(|r| r.session_id == "s3").unwrap();
    assert!(s3.bm25_rank.is_some());
    assert!(s3.vec_rank.is_none());

    let s4 = results.iter().find(|r| r.session_id == "s4").unwrap();
    assert!(s4.bm25_rank.is_none());
    assert!(s4.vec_rank.is_some());
}

#[test]
fn test_rrf_single_source_bm25_only() {
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

    let results = rrf::fuse(&bm25, &[], 1.0, 1.0, 60.0);

    assert_eq!(results.len(), 3);
    // Order should follow BM25 ranking (by position, not by rank value)
    assert_eq!(results[0].session_id, "a");
    assert_eq!(results[1].session_id, "b");
    assert_eq!(results[2].session_id, "c");

    // All should have bm25_rank but no vec_rank
    for r in &results {
        assert!(r.bm25_rank.is_some());
        assert!(r.vec_rank.is_none());
    }
}

#[test]
fn test_rrf_single_source_vec_only() {
    let vec = vec![
        VecResult {
            session_id: "x".into(),
            distance: 0.1,
        },
        VecResult {
            session_id: "y".into(),
            distance: 0.5,
        },
    ];

    let results = rrf::fuse(&[], &vec, 1.0, 1.0, 60.0);

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].session_id, "x");
    assert_eq!(results[1].session_id, "y");
}

#[test]
fn test_rrf_custom_k() {
    let bm25 = vec![FtsResult {
        session_id: "a".into(),
        rank: -5.0,
    }];
    let vec = vec![VecResult {
        session_id: "b".into(),
        distance: 0.1,
    }];

    // With k=0, rank 1 gets weight 1/1 = 1.0
    let results = rrf::fuse(&bm25, &vec, 1.0, 1.0, 0.0);
    // Both should have score = 1/1 = 1.0
    assert!((results[0].score - 1.0).abs() < 1e-6);
    assert!((results[1].score - 1.0).abs() < 1e-6);
}

#[test]
fn test_rrf_weight_dominance() {
    // When BM25 weight is much higher, BM25-only items should rank higher
    let bm25 = vec![FtsResult {
        session_id: "bm25_only".into(),
        rank: -5.0,
    }];
    let vec = vec![VecResult {
        session_id: "vec_only".into(),
        distance: 0.1,
    }];

    let results = rrf::fuse(&bm25, &vec, 100.0, 1.0, 60.0);
    assert_eq!(results[0].session_id, "bm25_only");

    let results = rrf::fuse(&bm25, &vec, 1.0, 100.0, 60.0);
    assert_eq!(results[0].session_id, "vec_only");
}
