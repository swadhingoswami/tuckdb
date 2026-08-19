use std::sync::Arc;
use tuckdb::embedding::MockEmbeddingProvider;
use tuckdb::embedding::cosine;
use tuckdb::embedding::dedup::SemanticDedup;
use tuckdb::{EmbeddingProvider, IncrementalEngine};

fn main() {
    println!("=== TuckDB-AI: Phase 10 — Semantic Deduplication ===");
    println!("Embedding-based candidates are reported; nothing is deleted automatically.");
    println!();

    let docs: Vec<(i64, &str)> = vec![
        (1, "What is RAII?"),
        (2, "Explain Resource Acquisition Is Initialization."),
        (3, "RAII binds resource lifetime to object lifetime."),
        (
            4,
            "Resource Acquisition Is Initialization binds lifetime to object lifetime.",
        ),
        (5, "What is a virtual function?"),
    ];

    let mut engine = IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()));
    for (id, content) in &docs {
        engine.ingest(*id, content);
    }

    println!("--- Illustrative pair (paraphrase, from the spec) ---");
    let a = text_of(&docs, 1);
    let b = text_of(&docs, 2);
    let provider = MockEmbeddingProvider::default();
    let score = cosine(&provider.embed(a), &provider.embed(b));
    println!("Document A: \"{}\"", a);
    println!("Document B: \"{}\"", b);
    println!("similarity = {:.2}", score);
    println!(
        "(above the 0.65 detector threshold with the mock, so this paraphrase pair is flagged)"
    );
    println!();

    let detector = SemanticDedup::new(0.65);
    let report = detector.detect(engine.store());
    println!(
        "--- Semantic duplicate candidates (similarity >= {:.2}, {} pairs compared) ---",
        detector.threshold(),
        report.pairs_compared
    );
    for m in &report.matches {
        println!();
        println!(
            "Document {}: \"{}\"",
            m.document_id,
            text_of(&docs, m.document_id)
        );
        println!(
            "Document {}: \"{}\"",
            m.other_document_id,
            text_of(&docs, m.other_document_id)
        );
        println!("similarity = {:.2}", m.similarity);
        println!("Possible semantic duplicate");
    }
    println!();
    println!("No candidates were deleted; safe deduplication policies come later.");

    let pairs: Vec<(i64, i64)> = report
        .matches
        .iter()
        .map(|m| (m.document_id, m.other_document_id))
        .collect();
    assert!(pairs.contains(&(2, 4)) && pairs.contains(&(3, 4)));
    assert!(!pairs.contains(&(1, 5)));
}

fn text_of<'a>(docs: &'a [(i64, &str)], id: i64) -> &'a str {
    docs.iter().find(|(d, _)| *d == id).unwrap().1
}
