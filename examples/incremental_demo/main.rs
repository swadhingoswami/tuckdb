use std::sync::Arc;
use std::time::Instant;
use tuckdb::IncrementalEngine;
use tuckdb::embedding::{EmbeddingProvider, MockEmbeddingProvider};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut scale = 10_000usize;
    let mut changed = 1_000usize;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--scale" => {
                i += 1;
                scale = args[i].parse().expect("--scale expects a number");
            }
            "--changed" => {
                i += 1;
                changed = args[i].parse().expect("--changed expects a number");
            }
            other => {
                eprintln!("unknown argument: {}", other);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    println!("=== TuckDB-AI: Phase 8 — Incremental Vector Update ===");
    println!("UPDATE document -> detect change -> re-embed ONLY changed chunks.");
    println!();
    println!(
        "scale = {} documents ({} chunks each), {} documents changed",
        scale, CHUNKS_PER_DOC, changed
    );
    println!();

    let mut engine = IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()));

    println!("Building the initial vector index...");
    let t0 = Instant::now();
    for id in 0..scale {
        engine.ingest(id as i64, &doc_content(id));
    }
    let ingest_elapsed = t0.elapsed();
    let total_chunks = engine.store().len();
    println!(
        "  embedded {} chunks in {} ms",
        total_chunks,
        ingest_elapsed.as_millis()
    );
    println!();

    println!("Updating {} documents (1 chunk each)...", changed);
    let t1 = Instant::now();
    let mut generated = 0usize;
    for id in 0..changed {
        let report = engine.update(id as i64, &updated_content(id)).unwrap();
        generated += report.embeddings_generated;
    }
    let update_elapsed = t1.elapsed();
    let work_avoided = (1.0 - generated as f64 / total_chunks as f64) * 100.0;

    println!();
    println!("Dataset:                 {:>8} chunks", total_chunks);
    println!("Changed:                 {:>8}", changed);
    println!("Processed:               {:>8}", generated);
    println!("Skipped:                 {:>8}", total_chunks - generated);
    println!();
    println!("Work avoided:            {:.1}%", work_avoided);
    println!(
        "Update phase elapsed:    {:>4} ms",
        update_elapsed.as_millis()
    );
    println!();
    println!(
        "Baseline (naive) would re-embed all {} chunks; incremental embedded only {}.",
        total_chunks, generated
    );
    println!(
        "For the full target: cargo run --example incremental_demo -- --scale 100000 --changed 1000"
    );
    println!("  -> 500,000 chunks, 1,000 processed, 499,000 skipped, ~99.8% avoided");
    println!();

    let provider = MockEmbeddingProvider::default();
    let qv = provider.embed("memory layout REVISED");
    let top = engine.store().search(&qv, 1);
    if let Some(m) = top.first() {
        println!(
            "Sanity: vector index is still searchable after updates (doc {}, chunk {}, score {:.3}).",
            m.document_id, m.chunk_id, m.score
        );
    }
    assert_eq!(engine.store().len(), total_chunks);
    assert_eq!(generated, changed);
}

const CHUNKS_PER_DOC: usize = 5;

fn doc_content(id: usize) -> String {
    format!(
        "Section 1 of document {id}: resource management.\n\nSection 2 of document {id}: memory layout.\n\nSection 3 of document {id}: concurrency notes.\n\nSection 4 of document {id}: performance.\n\nSection 5 of document {id}: summary."
    )
}

fn updated_content(id: usize) -> String {
    format!(
        "Section 1 of document {id}: resource management.\n\nSection 2 of document {id}: memory layout REVISED.\n\nSection 3 of document {id}: concurrency notes.\n\nSection 4 of document {id}: performance.\n\nSection 5 of document {id}: summary."
    )
}
