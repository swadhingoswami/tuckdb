use tuckdb::lifecycle::{ContentDedup, DedupOutcome};

fn main() {
    println!("=== TuckDB-AI: Phase 2 — Exact Deduplication ===");
    println!("Duplicate input is detected using content hashes and skipped.");
    println!();

    println!("--- Conceptual example ---");
    let mut dedup = ContentDedup::new();
    let doc_a = dedup.insert(&["What is RAII?"]);
    let doc_b = dedup.insert(&["What is RAII?"]);
    println!("Document A: \"What is RAII?\"");
    println!("Document B: \"What is RAII?\"");
    println!("same content hash -> A: {:?}, B: {:?}", doc_a, doc_b);
    match dedup.lookup(&["What is RAII?"]) {
        Some(canonical) => println!("canonical content shared: {}", canonical.join(" ")),
        None => unreachable!(),
    }
    println!("=> duplicate processing avoided");
    println!();

    println!("--- Benchmark: 10,000 records, 1,000 exact duplicates ---");
    let mut bench = ContentDedup::new();
    let total = 10_000usize;
    let duplicate_extra = 1_000usize;

    for i in 0..total - duplicate_extra {
        bench.insert(&[&format!("content_{}", i)]);
    }
    let mut duplicates_skipped = 0usize;
    for i in 0..duplicate_extra {
        if bench.insert(&[&format!("content_{}", i)]) == DedupOutcome::Duplicate {
            duplicates_skipped += 1;
        }
    }

    let report = bench.report();
    println!("Input rows:                 {:>6}", report.total_inputs);
    println!("Unique contents:            {:>6}", report.unique_contents);
    println!(
        "Duplicates:                 {:>6}",
        report.duplicates_detected
    );
    println!(
        "Duplicate ratio:            {:>5.0}%",
        report.duplicate_ratio * 100.0
    );
    println!(
        "Duplicate processing avoided: {:>6}",
        report.processing_avoided
    );
    println!();
    println!(
        "embedded/skipped -> processed {} documents instead of {}",
        report.unique_contents, report.total_inputs
    );
    assert_eq!(duplicates_skipped, 1_000);
    assert_eq!(report.duplicates_detected, 1_000);
}
