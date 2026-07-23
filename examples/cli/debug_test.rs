fn main() {
    let dir = std::path::PathBuf::from("/tmp/tuckdb_test");
    let mut table = tuckdb::Table::open("users", dir.clone());

    let plan = tuckdb::exec::logical_plan::LogicalPlan::scan("users")
        .filter(tuckdb::exec::expr::col("score").gt(tuckdb::exec::expr::lit_float(80.0)));

    let mut rs = table.execute(plan);
    while let Some(batch) = rs.next_batch() {
        println!("Rows: {}", batch.num_rows);
        for col in &batch.columns {
            print!("{} | ", col.field.name);
        }
        println!();
    }
}
