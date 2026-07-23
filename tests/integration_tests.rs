use std::path::PathBuf;
use tuckdb::{
    Column, ColumnData, DataType, Field, LogicalPlan, RecordBatch, Schema, Table, col, lit_int,
};

fn test_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(format!("/tmp/tuckdb_tests/{}", name));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn cleanup(name: &str) {
    let dir = PathBuf::from(format!("/tmp/tuckdb_tests/{}", name));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_create_table_and_insert() {
    let dir = test_dir("create_insert");
    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Float64, true),
    ]);

    let mut table = Table::create("test_insert", schema.clone(), dir.clone());
    assert_eq!(table.name(), "test_insert");
    assert_eq!(table.num_rows(), 0);

    let batch = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(schema.fields[0].clone(), ColumnData::Int64(vec![1, 2, 3])),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Float64(vec![10.0, 20.0, 30.0]),
            ),
        ],
    );
    table.insert_batch(batch);
    assert_eq!(table.num_rows(), 3);
    cleanup("create_insert");
}

#[test]
fn test_scan_all() {
    let dir = test_dir("scan_all");
    let schema = Schema::new(vec![Field::new("x", DataType::Int64, false)]);

    let mut table = Table::create("scan_test", schema.clone(), dir);
    let batch = RecordBatch::new(
        schema.clone(),
        vec![Column::new(
            schema.fields[0].clone(),
            ColumnData::Int64(vec![1, 2, 3, 4, 5]),
        )],
    );
    table.insert_batch(batch);

    let mut result_set = table.scan_all();
    let mut total = 0usize;
    while let Some(batch) = result_set.next_batch() {
        total += batch.num_rows;
    }
    assert_eq!(total, 5);
    cleanup("scan_all");
}

#[test]
fn test_multiple_batches() {
    let dir = test_dir("multi_batch");
    let schema = Schema::new(vec![Field::new("val", DataType::Int64, false)]);
    let mut table = Table::create("multi", schema.clone(), dir);

    for i in 0..3 {
        let data: Vec<i64> = (i * 10..(i + 1) * 10).collect();
        let batch = RecordBatch::new(
            schema.clone(),
            vec![Column::new(
                schema.fields[0].clone(),
                ColumnData::Int64(data),
            )],
        );
        table.insert_batch(batch);
    }

    assert_eq!(table.num_rows(), 30);
    let mut total = 0usize;
    let mut result_set = table.scan_all();
    while let Some(batch) = result_set.next_batch() {
        total += batch.num_rows;
    }
    assert_eq!(total, 30);
    cleanup("multi_batch");
}

#[test]
fn test_flush_and_open() {
    let dir = test_dir("flush_open");
    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, false),
    ]);

    let mut table = Table::create("persist_test", schema.clone(), dir.clone());
    let batch = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(schema.fields[0].clone(), ColumnData::Int64(vec![1, 2, 3])),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Utf8(vec!["a".into(), "b".into(), "c".into()]),
            ),
        ],
    );
    table.insert_batch(batch);
    table.flush();

    let table2 = Table::open("persist_test", dir);
    assert_eq!(table2.num_rows(), 3);

    let mut rs = table2.scan_all();
    let mut total = 0;
    while let Some(batch) = rs.next_batch() {
        total += batch.num_rows;
    }
    assert_eq!(total, 3);
    cleanup("flush_open");
}

#[test]
fn test_query_with_filter() {
    let dir = test_dir("filter");
    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("score", DataType::Float64, true),
    ]);

    let mut table = Table::create("filter_test", schema.clone(), dir);
    let batch = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(
                schema.fields[0].clone(),
                ColumnData::Int64(vec![1, 2, 3, 4, 5]),
            ),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Float64(vec![10.0, 20.0, 30.0, 40.0, 50.0]),
            ),
        ],
    );
    table.insert_batch(batch);

    let plan = LogicalPlan::scan("filter_test").filter(col("id").gt(lit_int(2)));
    let mut rs = table.execute(plan);
    let mut total = 0;
    while let Some(batch) = rs.next_batch() {
        total += batch.num_rows;
    }
    assert_eq!(total, 3);
    cleanup("filter");
}

#[test]
fn test_query_with_projection() {
    let dir = test_dir("proj");
    let schema = Schema::new(vec![
        Field::new("a", DataType::Int64, false),
        Field::new("b", DataType::Float64, true),
        Field::new("c", DataType::Utf8, false),
    ]);

    let mut table = Table::create("proj_test", schema.clone(), dir);
    let batch = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(schema.fields[0].clone(), ColumnData::Int64(vec![1, 2, 3])),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Float64(vec![1.0, 2.0, 3.0]),
            ),
            Column::new(
                schema.fields[2].clone(),
                ColumnData::Utf8(vec!["x".into(), "y".into(), "z".into()]),
            ),
        ],
    );
    table.insert_batch(batch);

    let plan = LogicalPlan::scan("proj_test").project(&["a", "c"]);
    let mut rs = table.execute(plan);
    let batch = rs.next_batch().unwrap();
    assert_eq!(batch.columns.len(), 2);
    assert_eq!(batch.schema.fields[0].name, "a");
    assert_eq!(batch.schema.fields[1].name, "c");
    cleanup("proj");
}

#[test]
fn test_aggregate() {
    let dir = test_dir("agg");
    let schema = Schema::new(vec![
        Field::new("cat", DataType::Utf8, false),
        Field::new("val", DataType::Float64, true),
    ]);

    let mut table = Table::create("agg_test", schema.clone(), dir);

    let batch1 = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(
                schema.fields[0].clone(),
                ColumnData::Utf8(vec!["a".into(), "b".into(), "a".into()]),
            ),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Float64(vec![1.0, 2.0, 3.0]),
            ),
        ],
    );
    let batch2 = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(
                schema.fields[0].clone(),
                ColumnData::Utf8(vec!["b".into(), "a".into()]),
            ),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Float64(vec![4.0, 5.0]),
            ),
        ],
    );
    table.insert_batch(batch1);
    table.insert_batch(batch2);

    let plan = LogicalPlan::scan("agg_test").aggregate(
        vec![(tuckdb::AggOp::Sum, "val", "total")],
        vec!["cat".to_string()],
    );

    let mut rs = table.execute(plan);
    let batch = rs.next_batch().unwrap();
    assert_eq!(batch.num_rows, 2);

    // Verify sums: a = 1.0+3.0+5.0 = 9.0, b = 2.0+4.0 = 6.0
    let cat_col = &batch.columns[0];
    let sum_col = &batch.columns[1];
    match (&cat_col.data, &sum_col.data) {
        (tuckdb::ColumnData::Utf8(cats), tuckdb::ColumnData::Float64(sums)) => {
            let mut results: std::collections::HashMap<&str, f64> =
                std::collections::HashMap::new();
            for (c, s) in cats.iter().zip(sums.iter()) {
                results.insert(c.as_str(), *s);
            }
            assert!((results.get("a").copied().unwrap_or(0.0) - 9.0).abs() < 0.001);
            assert!((results.get("b").copied().unwrap_or(0.0) - 6.0).abs() < 0.001);
        }
        _ => panic!("unexpected column types"),
    }
    cleanup("agg");
}
