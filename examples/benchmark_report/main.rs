use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use tuckdb::IncrementalEngine;
use tuckdb::embedding::{EmbeddingProvider, MockEmbeddingProvider};
use tuckdb::lifecycle::chunk_document;

const CHUNKS_PER_DOC: usize = 5;

fn main() {
    let scales: [usize; 3] = [1_000, 10_000, 100_000];
    let ratios: [f64; 5] = [0.01, 0.05, 0.10, 0.50, 1.00];
    let provider = MockEmbeddingProvider::default();

    println!("=== TuckDB-AI Benchmark Report ===");
    println!(
        "Mock embedding model-v1 (128 dims). Each document has {} chunks.",
        CHUNKS_PER_DOC
    );
    println!("Incremental update re-embeds only chunks whose content changed.");
    println!("Naive baseline = measured cost of re-embedding every chunk once.");
    println!();

    let mut out = String::new();
    out.push_str("# TuckDB-AI Benchmark Report\n\n");
    out.push_str(
        "Mock embedding model-v1 (128 dims). Each document has 5 chunks. The changed % is the \
         fraction of documents updated; each updated document changes all of its chunks, so it \
         equals the fraction of chunks whose content changed. Incremental update re-embeds only \
         the changed chunks. Naive baseline is the measured cost of re-embedding every chunk once.\n\n",
    );
    out.push_str(
        "| scale | chunks | changed % | incremental ms | embeddings regenerated | work avoided % | speedup vs naive | ingest ms | naive ms |\n",
    );
    out.push_str("|---|---|---|---|---|---|---|---|---|\n");

    println!(
        "{:<7} {:>9} {:>8} {:>12} {:>14} {:>13} {:>11} {:>10} {:>9}",
        "scale",
        "chunks",
        "changed",
        "incr(ms)",
        "regen",
        "avoided",
        "speedup",
        "ingest(ms)",
        "naive(ms)"
    );

    for &scale in &scales {
        let total_chunks = scale * CHUNKS_PER_DOC;
        let mut engine = IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()));

        let t = Instant::now();
        for id in 0..scale {
            engine.ingest(id as i64, &content(id));
        }
        let ingest_ms = t.elapsed().as_nanos() as f64 / 1e6;

        let t = Instant::now();
        for id in 0..scale {
            let chunks = chunk_document(id as i64, 1, &content(id));
            for chunk in &chunks {
                black_box(provider.embed(&chunk.content));
            }
        }
        let naive_ms = t.elapsed().as_nanos() as f64 / 1e6;

        let mut cursor = 0usize;
        let mut generated_total = 0usize;
        for &ratio in &ratios {
            let target = (scale as f64 * ratio).round() as usize;
            let t = Instant::now();
            let mut generated = 0usize;
            for id in cursor..target {
                let r = engine.update(id as i64, &updated(id)).unwrap();
                generated += r.embeddings_generated;
            }
            let incr_ms = t.elapsed().as_nanos() as f64 / 1e6;
            cursor = target;
            generated_total += generated;

            let avoided = (1.0 - generated_total as f64 / total_chunks as f64) * 100.0;
            let speedup = if incr_ms > 0.0 {
                naive_ms / incr_ms
            } else {
                f64::NAN
            };

            let row = format!(
                "| {} | {} | {:.0}% | {:.2} | {} | {:.1}% | {:.1}x | {:.0} | {:.0} |",
                scale,
                total_chunks,
                ratio * 100.0,
                incr_ms,
                generated_total,
                avoided,
                speedup,
                ingest_ms,
                naive_ms
            );
            out.push_str(&row);
            out.push('\n');
            println!(
                "{:<7} {:>9} {:>7.0}% {:>12.2} {:>14} {:>12.1}% {:>10.1}x {:>10.0} {:>9.0}",
                scale,
                total_chunks,
                ratio * 100.0,
                incr_ms,
                generated_total,
                avoided,
                speedup,
                ingest_ms,
                naive_ms
            );
        }
    }

    std::fs::create_dir_all("target").ok();
    let path = "target/benchmark_report.md";
    std::fs::write(path, &out).expect("failed to write benchmark report");
    println!();
    println!("Full markdown report written to {}", path);
}

fn content(id: usize) -> String {
    (0..CHUNKS_PER_DOC)
        .map(|c| {
            format!(
                "Section {} of document {}: topic text {}",
                c + 1,
                id,
                (id + c) % 10
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn updated(id: usize) -> String {
    (0..CHUNKS_PER_DOC)
        .map(|c| {
            format!(
                "Section {} of document {}: topic text {} REVISED",
                c + 1,
                id,
                (id + c) % 10
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
