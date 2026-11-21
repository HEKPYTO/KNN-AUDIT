use knn_audit_adapters::faiss::FaissEngine;
use knn_audit_adapters::pgvector::PgVectorEngine;
use knn_audit_adapters::qdrant::QdrantEngine;
use knn_audit_adapters::{CollectionView, Engine, Metric};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const QDRANT_URL: &str = "http://127.0.0.1:6333";
const DATABASE_URL: &str = "postgres://postgres:postgres@127.0.0.1:5432/postgres";

fn enabled() -> bool {
    if std::env::var("KNN_AUDIT_INTEGRATION").as_deref() == Ok("1") {
        true
    } else {
        println!("skipping: set KNN_AUDIT_INTEGRATION=1 to run engine integration tests");
        false
    }
}

fn faiss_python() -> Option<String> {
    if !enabled() {
        return None;
    }
    let python = std::env::var("KNN_AUDIT_FAISS_PYTHON").unwrap_or_else(|_| "python3".to_string());
    let available = Command::new(&python)
        .args(["-c", "import faiss"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !available {
        println!("skipping: faiss is not installed in {python}");
        return None;
    }
    Some(python)
}

fn unique_name(label: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    format!("audit_{label}_{}_{nanos}", std::process::id())
}

fn six_vectors() -> (Vec<u64>, Vec<f32>) {
    (
        vec![10, 20, 30, 40, 50, 60],
        vec![1.0, 0.0, 0.0, 2.0, 3.0, 0.0, 0.0, 0.0, 2.0, 2.0, 1.0, 1.0],
    )
}

fn check_order(hits: &[knn_audit_adapters::Hit]) {
    let ids: Vec<&str> = hits.iter().map(|hit| hit.id.as_str()).collect();
    assert_eq!(ids, ["40", "10", "60", "20", "50", "30"]);
    let expected = [0.0, 1.0, 2.0_f32.sqrt(), 2.0, 8.0_f32.sqrt(), 3.0];
    for (hit, distance) in hits.iter().zip(expected) {
        assert!((hit.distance.expect("l2 distance") - distance as f64).abs() < 1e-5);
    }
}

#[test]
fn qdrant_prepare_replaces_collection_and_reports_count() {
    if !enabled() {
        return;
    }
    let mut engine = QdrantEngine::new(QDRANT_URL, unique_name("qdrant_replace")).expect("engine");
    let (ids, vectors) = six_vectors();
    let view = CollectionView {
        dim: 2,
        ids: &ids,
        vectors: &vectors,
    };
    engine.prepare(&view).expect("first prepare");
    engine.prepare(&view).expect("second prepare");
    assert_eq!(engine.search(&[0.0, 0.0], 10).expect("search").len(), 6);
    assert!(engine.version().contains('.'));
    assert!(engine.capabilities().exact_mode);
    assert_eq!(engine.capabilities().distance_metric, Metric::L2);
    engine.reset().expect("cleanup");
}

#[test]
fn qdrant_prepare_rejects_count_mismatch() {
    if !enabled() {
        return;
    }
    let mut engine = QdrantEngine::new(QDRANT_URL, unique_name("qdrant_count")).expect("engine");
    let view = CollectionView {
        dim: 2,
        ids: &[1, 2, 3],
        vectors: &[0.0, 0.0, 1.0, 1.0],
    };
    let error = engine.prepare(&view).expect_err("count mismatch");
    assert!(error.message.contains("count mismatch"));
}

#[test]
fn qdrant_exact_search_hand_checked_order_and_k_above_size() {
    if !enabled() {
        return;
    }
    let mut engine = QdrantEngine::new(QDRANT_URL, unique_name("qdrant_order")).expect("engine");
    let (ids, vectors) = six_vectors();
    let view = CollectionView {
        dim: 2,
        ids: &ids,
        vectors: &vectors,
    };
    engine.prepare(&view).expect("prepare");
    check_order(&engine.search(&[0.0, 0.0], 10).expect("search"));
    engine.reset().expect("cleanup");
}

#[test]
fn qdrant_unreachable_endpoint_is_an_adapter_error() {
    if !enabled() {
        return;
    }
    let mut engine = QdrantEngine::new("http://127.0.0.1:1", "audit_dead").expect("engine");
    let view = CollectionView {
        dim: 2,
        ids: &[1],
        vectors: &[0.0, 0.0],
    };
    assert!(engine.prepare(&view).is_err());
}

#[test]
fn pgvector_prepare_replaces_table_and_reports_count() {
    if !enabled() {
        return;
    }
    let mut engine = PgVectorEngine::new(DATABASE_URL, unique_name("pg_replace")).expect("engine");
    let (ids, vectors) = six_vectors();
    let view = CollectionView {
        dim: 2,
        ids: &ids,
        vectors: &vectors,
    };
    engine.prepare(&view).expect("first prepare");
    engine.prepare(&view).expect("second prepare");
    assert_eq!(engine.search(&[0.0, 0.0], 10).expect("search").len(), 6);
    assert!(engine.version().contains('.'));
    assert!(engine.capabilities().exact_mode);
    assert_eq!(engine.capabilities().distance_metric, Metric::L2);
    engine.reset().expect("cleanup");
}

#[test]
fn pgvector_prepare_rejects_count_mismatch() {
    if !enabled() {
        return;
    }
    let mut engine = PgVectorEngine::new(DATABASE_URL, unique_name("pg_count")).expect("engine");
    let view = CollectionView {
        dim: 2,
        ids: &[1, 2, 3],
        vectors: &[0.0, 0.0, 1.0, 1.0],
    };
    let error = engine.prepare(&view).expect_err("count mismatch");
    assert!(error.message.contains("count mismatch"));
}

#[test]
fn pgvector_exact_search_hand_checked_order_and_k_above_size() {
    if !enabled() {
        return;
    }
    let mut engine = PgVectorEngine::new(DATABASE_URL, unique_name("pg_order")).expect("engine");
    let (ids, vectors) = six_vectors();
    let view = CollectionView {
        dim: 2,
        ids: &ids,
        vectors: &vectors,
    };
    engine.prepare(&view).expect("prepare");
    check_order(&engine.search(&[0.0, 0.0], 10).expect("search"));
    engine.reset().expect("cleanup");
}

#[test]
fn pgvector_unreachable_endpoint_is_an_adapter_error() {
    if !enabled() {
        return;
    }
    assert!(
        PgVectorEngine::new(
            "postgres://postgres:postgres@127.0.0.1:1/postgres?connect_timeout=1",
            "audit_dead"
        )
        .is_err()
    );
}

#[test]
fn qdrant_tie_heavy_prepare_keeps_the_same_id_set() {
    if !enabled() {
        return;
    }
    let ids: Vec<u64> = (0..128).collect();
    let mut vectors = vec![0.0; ids.len() * 4];
    for (index, vector) in vectors.chunks_mut(4).enumerate() {
        vector[index % 4] = 1.0;
    }
    let mut engine = QdrantEngine::new(QDRANT_URL, unique_name("qdrant_ties")).expect("engine");
    let view = CollectionView {
        dim: 4,
        ids: &ids,
        vectors: &vectors,
    };
    engine.prepare(&view).expect("first prepare");
    let first = engine.search(&[0.0; 4], ids.len()).expect("first search");
    engine.prepare(&view).expect("second prepare");
    let second = engine.search(&[0.0; 4], ids.len()).expect("second search");
    let mut first_ids: Vec<u64> = first
        .iter()
        .map(|hit| hit.id.parse().expect("numeric id"))
        .collect();
    let mut second_ids: Vec<u64> = second
        .iter()
        .map(|hit| hit.id.parse().expect("numeric id"))
        .collect();
    first_ids.sort_unstable();
    second_ids.sort_unstable();
    assert_eq!(first_ids, ids);
    assert_eq!(second_ids, first_ids);
    engine.reset().expect("cleanup");
}

#[test]
fn faiss_prepare_replaces_index_and_reports_count() {
    let Some(python) = faiss_python() else {
        return;
    };
    let mut engine = FaissEngine::new(python).expect("engine");
    let (ids, vectors) = six_vectors();
    let view = CollectionView {
        dim: 2,
        ids: &ids,
        vectors: &vectors,
    };
    engine.prepare(&view).expect("first prepare");
    engine.prepare(&view).expect("second prepare");
    assert_eq!(engine.search(&[0.0, 0.0], 10).expect("search").len(), 6);
    assert!(engine.version().contains('.'));
    assert!(engine.capabilities().exact_mode);
    assert_eq!(engine.capabilities().distance_metric, Metric::SquaredL2);
    engine.reset().expect("cleanup");
}

#[test]
fn faiss_exact_search_hand_checked_squared_order() {
    let Some(python) = faiss_python() else {
        return;
    };
    let mut engine = FaissEngine::new(python).expect("engine");
    let (ids, vectors) = six_vectors();
    let view = CollectionView {
        dim: 2,
        ids: &ids,
        vectors: &vectors,
    };
    engine.prepare(&view).expect("prepare");
    let hits = engine.search(&[0.0, 0.0], 10).expect("search");
    let got: Vec<&str> = hits.iter().map(|hit| hit.id.as_str()).collect();
    assert_eq!(got, ["40", "10", "60", "20", "50", "30"]);
    let expected = [0.0, 1.0, 2.0, 4.0, 8.0, 9.0];
    for (hit, distance) in hits.iter().zip(expected) {
        assert!((hit.distance.expect("squared l2") - distance).abs() < 1e-5);
    }
    engine.reset().expect("cleanup");
}
