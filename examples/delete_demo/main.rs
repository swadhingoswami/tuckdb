use std::sync::Arc;
use std::time::Instant;
use tuckdb::IncrementalEngine;
use tuckdb::embedding::{EmbeddingProvider, MockEmbeddingProvider};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut docs = 10_000usize;
    let mut chunks_per_doc = 3usize;
    let mut to_delete = 100usize;
    let mut delete_start = 100usize;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--docs" => {
                i += 1;
                docs = args[i].parse().expect("--docs expects a number");
            }
            "--chunks" => {
                i += 1;
                chunks_per_doc = args[i].parse().expect("--chunks expects a number");
            }
            "--delete" => {
                i += 1;
                to_delete = args[i].parse().expect("--delete expects a number");
            }
            "--start" => {
                i += 1;
                delete_start = args[i].parse().expect("--start expects a number");
            }
            other => {
                eprintln!("unknown argument: {}", other);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    println!("=== TuckDB-AI: Phase 9 — Delete / Stale Cleanup ===");
    println!("Deleting a source row cleans only its derived data.");
    println!();

    let mut engine = IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()));
    for id in 0..docs {
        engine.ingest(id as i64, &doc_content(id, chunks_per_doc));
    }
    let total_before = engine.store().len();
    println!("Before: documents = {}, vectors = {}", docs, total_before);
    println!(
        "Delete: documents {}..={} ({} documents)",
        delete_start,
        delete_start + to_delete - 1,
        to_delete
    );
    println!();

    let t0 = Instant::now();
    let mut affected_vectors = 0usize;
    for id in delete_start..delete_start + to_delete {
        let report = engine.delete(id as i64).unwrap();
        affected_vectors += report.affected_vectors;
    }
    let elapsed = t0.elapsed();
    let total_after = engine.store().len();

    println!("Affected vectors:              {:>8}", affected_vectors);
    println!("Total vectors (after):         {:>8}", total_after);
    println!("Cleanup work (direct removals):  {:>8}", affected_vectors);
    println!("Unrelated vectors untouched:   {:>8}", total_after);
    println!(
        "Delete phase elapsed:          {:>8} ms",
        elapsed.as_millis()
    );
    println!();
    println!(
        "Cleanup is O(chunks in the document), not O(total vectors) - no full scan of the vector dataset."
    );

    let provider = MockEmbeddingProvider::default();
    let untouched_id = 50usize;
    let qv = provider.embed(&doc_content(untouched_id, chunks_per_doc));
    let top = engine.store().search(&qv, 1);
    println!();
    match top.first() {
        Some(m) if m.document_id == untouched_id as i64 => {
            println!(
                "Sanity: untouched document {} is still searchable (score {:.3}).",
                untouched_id, m.score
            );
        }
        _ => panic!("untouched document should remain searchable"),
    }

    assert_eq!(total_after, total_before - affected_vectors);
    assert_eq!(affected_vectors, to_delete * chunks_per_doc);
}

fn doc_content(id: usize, chunks: usize) -> String {
    (0..chunks)
        .map(|c| format!("Section {} of document {}: text {}", c + 1, id, c))
        .collect::<Vec<_>>()
        .join("\n\n")
}
