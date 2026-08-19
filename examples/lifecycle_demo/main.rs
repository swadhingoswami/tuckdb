use std::path::PathBuf;
use tuckdb::lifecycle::{LifecycleManager, LifecycleState};
use tuckdb::{Column, ColumnData, DataType, Field, RecordBatch, Schema, Table};

fn main() {
    let dir = PathBuf::from("/tmp/tuckdb_lifecycle");
    std::fs::create_dir_all(&dir).unwrap();

    println!("=== TuckDB-AI: Phase 1 — Simple Lifecycle Metadata ===");
    println!("A small change in AI data should not trigger expensive reprocessing.");
    println!();

    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("question", DataType::Utf8, false),
        Field::new("answer", DataType::Utf8, false),
        Field::new("topic", DataType::Utf8, false),
    ]);
    let mut table = Table::create("cpp_questions", schema.clone(), dir);

    let q1 = "What is RAII?";
    let a1 = "RAII binds resource lifetime to object lifetime.";
    let a1_updated = "RAII acquires resources in constructors and releases them in destructors.";
    let t1 = "memory";

    let batch = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(schema.fields[0].clone(), ColumnData::Int64(vec![1])),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Utf8(vec![q1.to_string()]),
            ),
            Column::new(
                schema.fields[2].clone(),
                ColumnData::Utf8(vec![a1.to_string()]),
            ),
            Column::new(
                schema.fields[3].clone(),
                ColumnData::Utf8(vec![t1.to_string()]),
            ),
        ],
    );
    table.insert_batch(batch);
    table.flush();

    let mut lm = LifecycleManager::new();

    println!("--- INSERT document 1 ---");
    lm.register(1, &[q1, a1, t1]);
    let s = lm.get(1).unwrap();
    println!("document_id = {}", s.document_id);
    println!("version     = {}", s.version);
    println!("content_hash = {:016x}", s.content_hash);
    println!("state       = {:?}", s.state);
    println!("source row stored in 'cpp_questions' table");
    println!();

    println!("--- UPDATE document 1 (identical content) ---");
    let report = lm.apply_update(1, &[q1, a1, t1]).unwrap();
    println!("Previous version: {}", report.previous_version);
    println!("New version:      {}", report.new_version);
    println!(
        "Content changed:  {}",
        if report.content_changed { "YES" } else { "NO" }
    );
    println!("Derived state:    {:?}", report.derived_state);
    println!("=> old_hash == new_hash: NO downstream work");
    println!();

    println!("--- UPDATE document 1 (changed answer) ---");
    println!("UPDATE document 1");
    println!();
    let report = lm.apply_update(1, &[q1, a1_updated, t1]).unwrap();
    println!("Previous version: {}", report.previous_version);
    println!("New version:      {}", report.new_version);
    println!();
    println!(
        "Content changed:  {}",
        if report.content_changed { "YES" } else { "NO" }
    );
    println!("Derived state:    {:?}", report.derived_state);
    println!();
    println!("=> source version incremented, derived representation marked STALE");
    println!("=> future lifecycle processing will act only on this document");

    let state = lm.get(1).unwrap();
    assert_eq!(state.version, 2);
    assert_eq!(state.state, LifecycleState::Stale);
}
