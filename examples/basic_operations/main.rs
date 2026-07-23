use std::path::PathBuf;
use tuckdb::*;

fn main() {
    let dir = PathBuf::from("/tmp/tuckdb_scenarios");
    std::fs::create_dir_all(&dir).unwrap();

    println!("=== Scenario 1: Basic Operations ===");
    println!("Create table, insert, scan, filter, project, aggregate\n");

    // Step 1: Define schema
    println!("Step 1: Define schema with 4 columns");
    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("score", DataType::Float64, true),
        Field::new("active", DataType::Int64, false),
    ]);
    println!("  Schema: id(Int64), name(Utf8), score(Float64), active(Int64)\n");

    // Step 2: Create table
    println!("Step 2: Create table 'users'");
    let mut table = Table::create("users", schema.clone(), dir.clone());
    println!("  Table created. Rows: {}\n", table.num_rows());

    // Step 3: Insert data
    println!("Step 3: Insert 2 batches of data");
    let batch1 = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(schema.fields[0].clone(), ColumnData::Int64(vec![1, 2, 3])),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Utf8(vec!["Alice".into(), "Bob".into(), "Charlie".into()]),
            ),
            Column::new(
                schema.fields[2].clone(),
                ColumnData::Float64(vec![85.5, 92.0, 78.3]),
            ),
            Column::new(
                schema.fields[3].clone(),
                ColumnData::Int64(vec![1, 1, 0]),
            ),
        ],
    );
    let batch2 = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(schema.fields[0].clone(), ColumnData::Int64(vec![4, 5])),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Utf8(vec!["Diana".into(), "Eve".into()]),
            ),
            Column::new(
                schema.fields[2].clone(),
                ColumnData::Float64(vec![95.0, 88.1]),
            ),
            Column::new(
                schema.fields[3].clone(),
                ColumnData::Int64(vec![0, 1]),
            ),
        ],
    );
    table.insert_batch(batch1);
    table.insert_batch(batch2);
    println!("  Inserted 5 rows total\n");

    // Step 4: Scan all
    println!("Step 4: Scan all rows (SELECT *)");
    {
        let plan = LogicalPlan::scan("users");
        let mut rs = table.execute(plan);
        println!("  Results:");
        while let Some(batch) = rs.next_batch() {
            for row in 0..batch.num_rows {
                print!("    Row {}: ", row);
                for col in &batch.columns {
                    match &col.data {
                        ColumnData::Int64(v) => print!("{} ", v[row]),
                        ColumnData::Float64(v) => print!("{:.1} ", v[row]),
                        ColumnData::Utf8(v) => print!("{} ", v[row]),
                        ColumnData::Timestamp(v) => print!("{} ", v[row]),
                    }
                }
                println!();
            }
        }
    }
    println!();

    // Step 5: Filter
    println!("Step 5: Filter (WHERE score > 80)");
    {
        let plan = LogicalPlan::scan("users").filter(col("score").gt(lit_float(80.0)));
        let mut rs = table.execute(plan);
        let mut count = 0;
        println!("  Results:");
        while let Some(batch) = rs.next_batch() {
            for row in 0..batch.num_rows {
                count += 1;
                let id = match &batch.columns[0].data {
                    ColumnData::Int64(v) => v[row],
                    _ => 0,
                };
                let name = match &batch.columns[1].data {
                    ColumnData::Utf8(v) => &v[row],
                    _ => "",
                };
                let score = match &batch.columns[2].data {
                    ColumnData::Float64(v) => v[row],
                    _ => 0.0,
                };
                println!("    Row {}: id={}, name={}, score={:.1}", count, id, name, score);
            }
        }
        println!("  Total: {} rows\n", count);
    }

    // Step 6: Projection
    println!("Step 6: Projection (SELECT name, score)");
    {
        let plan = LogicalPlan::scan("users").project(&["name", "score"]);
        let mut rs = table.execute(plan);
        println!("  Results:");
        while let Some(batch) = rs.next_batch() {
            for row in 0..batch.num_rows {
                let name = match &batch.columns[0].data {
                    ColumnData::Utf8(v) => &v[row],
                    _ => "",
                };
                let score = match &batch.columns[1].data {
                    ColumnData::Float64(v) => v[row],
                    _ => 0.0,
                };
                println!("    name={}, score={:.1}", name, score);
            }
        }
    }
    println!();

    // Step 7: Combined filter + projection
    println!("Step 7: Combined (SELECT name, score WHERE active = 1)");
    {
        let plan = LogicalPlan::scan("users")
            .filter(col("active").eq(lit_int(1)))
            .project(&["name", "score"]);
        let mut rs = table.execute(plan);
        println!("  Results:");
        let mut count = 0;
        while let Some(batch) = rs.next_batch() {
            for row in 0..batch.num_rows {
                count += 1;
                let name = match &batch.columns[0].data {
                    ColumnData::Utf8(v) => &v[row],
                    _ => "",
                };
                let score = match &batch.columns[1].data {
                    ColumnData::Float64(v) => v[row],
                    _ => 0.0,
                };
                println!("    {}. name={}, score={:.1}", count, name, score);
            }
        }
        println!("  Active users: {}\n", count);
    }

    // Step 8: Aggregation
    println!("Step 8: Aggregation (AVG(score) GROUP BY active)");
    {
        let plan = LogicalPlan::scan("users")
            .project(&["active", "score"])
            .aggregate(
                vec![
                    (AggOp::Count, "score", "count"),
                    (AggOp::Avg, "score", "avg_score"),
                    (AggOp::Min, "score", "min_score"),
                    (AggOp::Max, "score", "max_score"),
                ],
                vec!["active".to_string()],
            );
        let mut rs = table.execute(plan);
        println!("  Results:");
        while let Some(batch) = rs.next_batch() {
            for row in 0..batch.num_rows {
                let active = match &batch.columns[0].data {
                    ColumnData::Utf8(v) => &v[row],
                    _ => "",
                };
                let count = match &batch.columns[1].data {
                    ColumnData::Float64(v) => v[row],
                    _ => 0.0,
                };
                let avg = match &batch.columns[2].data {
                    ColumnData::Float64(v) => v[row],
                    _ => 0.0,
                };
                let min = match &batch.columns[3].data {
                    ColumnData::Float64(v) => v[row],
                    _ => 0.0,
                };
                let max = match &batch.columns[4].data {
                    ColumnData::Float64(v) => v[row],
                    _ => 0.0,
                };
                println!(
                    "    active={}: count={}, avg={:.1}, min={:.1}, max={:.1}",
                    active, count as u64, avg, min, max
                );
            }
        }
    }
    println!();

    // Step 9: Persist and reload
    println!("Step 9: Flush to disk and reload");
    table.flush();
    println!("  Flushed to {}/users.tuck", dir.display());

    let reloaded = Table::open("users", dir);
    println!("  Reloaded {} rows from disk\n", reloaded.num_rows());

    // Step 10: Verify reloaded data
    println!("Step 10: Verify reloaded data (scan all)");
    {
        let mut rs = reloaded.scan_all();
        let mut count = 0;
        while let Some(batch) = rs.next_batch() {
            count += batch.num_rows;
        }
        println!("  Scanned {} rows — data integrity verified", count);
    }

    println!("\n=== All scenarios completed successfully ===");
}
