use std::sync::Arc;
use std::time::Instant;

use tuckdb::IncrementalEngine;
use tuckdb::embedding::{EmbeddingProvider, MockEmbeddingProvider};

const DEMO_FILE: &str = "data/demo_cpp.txt";
const UPDATED_FILE: &str = "data/demo_cpp_v2.txt";
const EXTRA_FILE: &str = "README.md";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("==========================================================");
    println!("  TuckDB-AI | Proof of Concept");
    println!("  Real file -> chunk -> embed -> vector DB -> query -> incremental update/delete");
    println!("==========================================================");
    println!();

    let mut engine = IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()));
    let provider = MockEmbeddingProvider::default();

    // ------------------------------------------------------------------ [1]
    println!("[1] Take a real file");
    let text = std::fs::read_to_string(DEMO_FILE)?;
    println!(
        "    file: {DEMO_FILE} ({} bytes, {} paragraphs)",
        text.len(),
        text.split("\n\n").count()
    );
    println!();

    // ------------------------------------------------------------ [2] + [3]
    println!("[2] Upload it into the TuckDB vector DB (chunk -> embed -> store)");
    let t = Instant::now();
    let ingest = engine.ingest(0, &text);
    let insert_us = t.elapsed().as_micros();
    let chunks = engine.chunks().get(0).unwrap();
    println!("    chunks created: {}", ingest.chunks);
    for c in chunks {
        let head: String = c.content.chars().take(42).collect();
        println!(
            "      chunk {:>2}: hash {:016x}  \"{}\"...",
            c.chunk_id, c.content_hash, head
        );
    }
    println!();

    println!("[3] Count + timing benchmark (insert)");
    println!("    chunks:               {:>4}", ingest.chunks);
    println!("    embeddings generated: {:>4}", ingest.chunks);
    println!("    vectors stored:       {:>4}", engine.store().len());
    println!(
        "    insert time:           {:>6} us ({:.2} ms)",
        insert_us,
        insert_us as f64 / 1000.0
    );
    println!();

    // ------------------------------------------------------------------ [4]
    println!("[4] Query related info from the uploaded file");
    let queries = [
        "How do I manage ownership of dynamically allocated memory?",
        "What happens when an object goes out of scope?",
        "What is runtime polymorphism?",
    ];
    for q in queries {
        let qv = provider.embed(q);
        let m = &engine.store().search(&qv, 1)[0];
        let chunks = engine.chunks().get(m.document_id).unwrap();
        let content = &chunks[m.chunk_id as usize].content;
        let head: String = content.chars().take(90).collect();
        println!("    Q: \"{q}\"");
        println!(
            "      -> chunk {} score={:.3} \"{}\"...",
            m.chunk_id, m.score, head
        );
    }
    println!();

    // ------------------------------------------------------------------ [5]
    println!("[5] Add an extra file (incremental: only the NEW file is processed)");
    let extra = std::fs::read_to_string(EXTRA_FILE)?;
    let before = engine.store().len();
    let t = Instant::now();
    let add = engine.ingest(1, &extra);
    let add_ms = t.elapsed().as_millis();
    println!(
        "    add {EXTRA_FILE}: +{} chunks ({} -> {} vectors) in {} ms",
        add.chunks,
        before,
        engine.store().len(),
        add_ms
    );
    println!(
        "    existing doc 0 vectors untouched: {}",
        engine.store().get(0, 0).is_some()
    );
    println!();

    println!("[5b] Update doc 0 (edit 1 paragraph -> only 1 chunk re-embedded)");
    let v2 = std::fs::read_to_string(UPDATED_FILE)?;
    let before = engine.store().len();
    let t = Instant::now();
    let upd = engine.update(0, &v2)?;
    let upd_ms = t.elapsed().as_millis();
    println!(
        "    update: changed = {}, re-embedded = {}, skipped = {} ({:.1}% avoided) in {} ms",
        upd.changed_chunks,
        upd.embeddings_generated,
        upd.embeddings_skipped,
        upd.work_avoided_pct,
        upd_ms
    );
    println!(
        "    vector count unchanged: {} -> {}",
        before,
        engine.store().len()
    );
    println!();

    println!("[5c] Delete the extra file (old data removed, only its vectors)");
    let before = engine.store().len();
    let t = Instant::now();
    let del = engine.delete(1)?;
    let del_ms = t.elapsed().as_millis();
    println!(
        "    delete: {} vectors removed ({} -> {}) in {} ms; doc 0 still searchable: {}",
        del.affected_vectors,
        before,
        engine.store().len(),
        del_ms,
        engine.store().get(0, 0).is_some()
    );
    println!();

    assert_eq!(ingest.chunks, 11);
    assert_eq!(engine.store().len(), 11);
    assert_eq!(
        upd.embeddings_generated, 1,
        "only 1 chunk should be re-embedded"
    );
    assert!(engine.store().get(0, 0).is_some(), "doc 0 must survive");
    assert!(
        engine.store().get(1, 0).is_none(),
        "deleted doc 1 must be gone"
    );
    println!("All checks passed.");
    Ok(())
}
