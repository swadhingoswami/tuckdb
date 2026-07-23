use std::path::PathBuf;
use tuckdb::{
    Column, ColumnData, DataType, Field, LogicalPlan, RecordBatch, Schema, Table, col, lit_float,
    lit_str,
};

fn main() {
    let dir = PathBuf::from("/tmp/tuckdb_demo");
    std::fs::create_dir_all(&dir).unwrap();

    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("score", DataType::Float64, true),
        Field::new("tag", DataType::Utf8, false),
    ]);

    let mut table = Table::create("demo", schema.clone(), dir.clone());

    // Ingest 10 batches of 10K rows
    for batch_idx in 0..10 {
        let n = 10_000;
        let ids: Vec<i64> = (batch_idx * n..(batch_idx + 1) * n).collect();
        let names: Vec<String> = (batch_idx * n..(batch_idx + 1) * n)
            .map(|i| format!("user_{}", i))
            .collect();
        let scores: Vec<f64> = (batch_idx * n..(batch_idx + 1) * n)
            .map(|i| (i as f64) * 0.5)
            .collect();
        let tags: Vec<String> = (batch_idx * n..(batch_idx + 1) * n)
            .map(|i| {
                if i % 3 == 0 {
                    "web"
                } else if i % 3 == 1 {
                    "mobile"
                } else {
                    "api"
                }
                .to_string()
            })
            .collect();

        let batch = RecordBatch::new(
            schema.clone(),
            vec![
                Column::new(schema.fields[0].clone(), ColumnData::Int64(ids)),
                Column::new(schema.fields[1].clone(), ColumnData::Utf8(names)),
                Column::new(schema.fields[2].clone(), ColumnData::Float64(scores)),
                Column::new(schema.fields[3].clone(), ColumnData::Utf8(tags)),
            ],
        );
        table.insert_batch(batch);
    }

    println!("=== TuckDB Demo ===");
    println!("Total rows ingested: {}", table.num_rows());

    // Scan all
    println!("\n--- Scan all ---");
    let plan = LogicalPlan::scan("demo");
    let mut rs = table.execute(plan);
    let mut count = 0usize;
    while let Some(batch) = rs.next_batch() {
        count += batch.num_rows;
    }
    println!("Scanned {} rows", count);

    // Filter
    println!("\n--- Filter (score > 25000) ---");
    let plan = LogicalPlan::scan("demo").filter(col("score").gt(lit_float(25000.0)));
    let mut rs = table.execute(plan);
    let mut count = 0usize;
    while let Some(batch) = rs.next_batch() {
        count += batch.num_rows;
    }
    println!("Found {} rows with score > 25000", count);

    // Projection
    println!("\n--- Project (id, name, tag) ---");
    let plan = LogicalPlan::scan("demo")
        .project(&["id", "name", "tag"])
        .filter(col("tag").eq(lit_str("web")))
        .aggregate(
            vec![(tuckdb::AggOp::Count, "id", "cnt")],
            vec!["tag".to_string()],
        );
    let mut rs = table.execute(plan);
    if let Some(batch) = rs.next_batch() {
        println!(
            "web entries: {} (from {})",
            batch.columns[1].data.len(),
            batch.num_rows
        );
    }

    // Flush to disk
    println!("\n--- Flush to disk ---");
    table.flush();
    println!("Table persisted to {}/demo.tuck", dir.display());

    // Re-open
    println!("\n--- Re-open from disk ---");
    let table2 = Table::open("demo", dir);
    println!("Re-opened table with {} rows", table2.num_rows());
}
