use std::sync::Arc;
use tuckdb::IncrementalEngine;
use tuckdb::embedding::{EmbeddingProvider, MockEmbeddingProvider};

fn main() {
    println!("=== TuckDB-AI: Phase 11 — Embedding Model Versioning ===");
    println!("Every vector knows its model; migration re-embeds only what changed.");
    println!();

    let mut engine = IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()));
    let docs = 100usize;
    for id in 0..docs {
        engine.ingest(id as i64, &doc_content(id));
    }
    let total = engine.store().len();
    println!(
        "Index: {} documents x {} chunks = {} vectors (model \"model-v1\")",
        docs, CHUNKS_PER_DOC, total
    );
    let before = engine.store().get(20, 0).unwrap();
    println!(
        "Example vector before migration: chunk (20,0) source_version={} model=\"{}\"",
        before.source_version, before.model_id
    );
    println!();

    let v2 = Arc::new(MockEmbeddingProvider::new("model-v2", 128));
    let report = engine.migrate_model(v2.clone());
    println!("Migrating {} -> {} ...", report.old_model, report.new_model);
    println!("  total vectors:        {:>6}", report.total_vectors);
    println!("  migration candidates: {:>6}", report.candidates);
    println!("  migrated:             {:>6}", report.migrated);
    println!("  skipped (already v2): {:>6}", report.skipped);
    let after = engine.store().get(20, 0).unwrap();
    println!();
    println!(
        "Example vector after migration:  chunk (20,0) source_version={} model=\"{}\"",
        after.source_version, after.model_id
    );
    println!("=> source version preserved; only the representation migrated.");
    println!();

    let again = engine.migrate_model(v2.clone());
    println!(
        "Migrating {} -> {} again ...",
        again.old_model, again.new_model
    );
    println!("  candidates: {:>6}", again.candidates);
    println!("  migrated:   {:>6}", again.migrated);
    println!("  skipped:    {:>6}", again.skipped);
    println!("=> no blind rebuild when the model is already current.");

    let qv = v2.embed("Section 2 of document 5: memory layout REVISED");
    let top = engine.store().search(&qv, 1);
    println!();
    match top.first() {
        Some(m) => println!(
            "Sanity: search after migration still works (doc {}, chunk {}, score {:.3}).",
            m.document_id, m.chunk_id, m.score
        ),
        None => println!("Sanity: no matches."),
    }

    assert_eq!(report.migrated, total);
    assert_eq!(again.skipped, total);
    assert!(engine.store().iter().all(|e| e.model_id == "model-v2"));
}

const CHUNKS_PER_DOC: usize = 5;

fn doc_content(id: usize) -> String {
    format!(
        "Section 1 of document {id}: resource management.\n\nSection 2 of document {id}: memory layout.\n\nSection 3 of document {id}: concurrency notes.\n\nSection 4 of document {id}: performance.\n\nSection 5 of document {id}: summary."
    )
}
