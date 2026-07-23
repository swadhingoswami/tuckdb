use std::path::PathBuf;
use tuckdb::*;

fn print_table(rs: &mut tuckdb::api::result::ResultSet, label: &str) {
    println!("  {}", label);
    let mut row_num = 0;
    while let Some(batch) = rs.next_batch() {
        for row in 0..batch.num_rows {
            row_num += 1;
            print!("    {}. ", row_num);
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
    if row_num == 0 {
        println!("    (no rows)");
    }
}

fn main() {
    let dir = PathBuf::from("/tmp/tuckdb_scenarios");
    std::fs::create_dir_all(&dir).unwrap();

    println!("=== Scenario 3: CRUD Workflow ===");
    println!("Full create, read, update (insert more), delete (filter), persist cycle\n");

    // CREATE
    println!("--- CREATE ---");
    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("product", DataType::Utf8, false),
        Field::new("price", DataType::Float64, true),
        Field::new("stock", DataType::Int64, false),
    ]);
    let mut table = Table::create("inventory", schema.clone(), dir.clone());

    let batch = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(schema.fields[0].clone(), ColumnData::Int64(vec![101, 102, 103])),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Utf8(vec!["Widget".into(), "Gadget".into(), "Doohickey".into()]),
            ),
            Column::new(
                schema.fields[2].clone(),
                ColumnData::Float64(vec![9.99, 24.99, 49.99]),
            ),
            Column::new(schema.fields[3].clone(), ColumnData::Int64(vec![100, 50, 25])),
        ],
    );
    table.insert_batch(batch);
    println!("  Created 'inventory' table with 3 products\n");

    // READ (initial)
    println!("--- READ (initial) ---");
    print_table(&mut table.scan_all(), "All products:");
    println!();

    // INSERT more
    println!("--- INSERT more data ---");
    let batch2 = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(schema.fields[0].clone(), ColumnData::Int64(vec![104, 105])),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Utf8(vec!["Thingamajig".into(), "Whatchamacallit".into()]),
            ),
            Column::new(
                schema.fields[2].clone(),
                ColumnData::Float64(vec![14.99, 34.99]),
            ),
            Column::new(schema.fields[3].clone(), ColumnData::Int64(vec![10, 5])),
        ],
    );
    table.insert_batch(batch2);
    println!("  Inserted 2 more products\n");
    print_table(&mut table.scan_all(), "All products after insert:");
    println!();

    // READ with filter (expensive products)
    println!("--- READ with filter (price > 20) ---");
    {
        let plan = LogicalPlan::scan("inventory")
            .filter(col("price").gt(lit_float(20.0)))
            .project(&["product", "price", "stock"]);
        let mut rs = table.execute(plan);
        print_table(&mut rs, "Products with price > $20:");
    }
    println!();

    // Simulate "delete" by filtering
    println!("--- Simulated DELETE (stock < 15) ---");
    {
        let plan = LogicalPlan::scan("inventory")
            .filter(col("stock").lt(lit_int(15)))
            .project(&["product", "stock"]);
        let mut rs = table.execute(plan);
        print_table(&mut rs, "Low-stock products (stock < 15):");
    }
    println!();

    // AGGREGATE (total inventory value)
    println!("--- AGGREGATE (inventory value) ---");
    {
        let plan = LogicalPlan::scan("inventory")
            .project(&["product", "price", "stock"])
            .aggregate(
                vec![
                    (AggOp::Count, "product", "product_count"),
                    (AggOp::Sum, "stock", "total_stock"),
                ],
                vec![],
            );
        let mut rs = table.execute(plan);
        print_table(&mut rs, "Inventory summary:");
    }
    println!();

    // PERSIST
    println!("--- PERSIST to disk ---");
    table.flush();
    println!("  Flushed to disk\n");

    // REOPEN
    println!("--- REOPEN from disk ---");
    let reloaded = Table::open("inventory", dir);
    println!("  Reloaded {} products\n", reloaded.num_rows());
    print_table(&mut reloaded.scan_all(), "Data from disk:");
    println!();

    println!("=== CRUD workflow completed ===");
}
