use tuckdb::lifecycle::{ChunkTracker, LifecycleManager, LifecycleState};

fn main() {
    let v1 = "Q1: What is RAII?
A1: RAII binds resource lifetime to object lifetime.

Q2: What is a move constructor?
A2: A move constructor transfers resources from another object.

Q3: What is a virtual function?
A3: A virtual function enables runtime polymorphism.";

    let v2 = "Q1: What is RAII?
A1: RAII binds resource lifetime to object lifetime.

Q2: What is a move constructor?
A2: A move constructor transfers ownership by moving resources instead of copying them.

Q3: What is a virtual function?
A3: A virtual function enables runtime polymorphism.";

    println!("=== TuckDB-AI: Phase 3 — Chunk Lifecycle ===");
    println!("Only the changed chunk requires downstream processing.");
    println!();

    let mut lm = LifecycleManager::new();
    let mut tracker = ChunkTracker::new();

    println!("--- Register document (version 1) ---");
    lm.register(1, &[v1]);
    let chunks = tracker.register(1, 1, v1);
    println!("Document V1 -> {} chunks", chunks.len());
    for chunk in &chunks {
        println!(
            "  chunk {}: hash {:016x}",
            chunk.chunk_id, chunk.content_hash
        );
    }
    println!();

    println!("--- Update: modify only Q2/A2 ---");
    let change = lm.apply_update(1, &[v2]).unwrap();
    println!(
        "lifecycle: version {} -> {}, state {:?}, content_changed = {}",
        change.previous_version,
        change.new_version,
        change.derived_state,
        if change.content_changed { "YES" } else { "NO" }
    );
    println!();

    let report = tracker.apply_update(1, change.new_version, v2).unwrap();
    println!("Per-chunk decisions:");
    for diff in &report.diffs {
        let label = match diff.change {
            tuckdb::ChunkChange::Skip => "SKIP",
            tuckdb::ChunkChange::Process => "PROCESS",
        };
        println!(
            "  chunk {} -> {} (hash {:016x} -> {:016x})",
            diff.chunk_id, label, diff.previous_hash, diff.new_hash
        );
    }
    println!();

    println!("Total chunks:             {:>4}", report.total_chunks);
    println!("Changed chunks:           {:>4}", report.changed_chunks);
    println!("Unchanged chunks:         {:>4}", report.unchanged_chunks);
    println!("Reprocessed chunks:       {:>4}", report.reprocessed_chunks);
    println!(
        "Processing avoided:       {:.1}%",
        report.processing_avoided_pct
    );

    let stale = lm.get(1).unwrap();
    assert_eq!(stale.version, 2);
    assert_eq!(stale.state, LifecycleState::Stale);
    assert_eq!(report.reprocessed_chunks, 1);
}
