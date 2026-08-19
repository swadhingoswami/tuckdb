# TuckDB Usage Guide

Complete step-by-step scenarios with commands and expected output.

> 💡 **New — TuckDB-AI:** semantic vector search, `SIMILARITY(...)`/`VECTOR_SEARCH(...)`
> SQL, and an incremental AI data lifecycle (chunks, embeddings, vector DB) that is
> derived from your data and updated incrementally. See
> [docs/VECTOR_DB.md](docs/VECTOR_DB.md) and the
> [README TuckDB-AI section](../README.md#-tuckdb-ai--incremental-ai-data-engine).

---

## Table of Contents

1. [Quick Start](#1-quick-start)
2. [Basic Operations](#2-basic-operations)
3. [Filter Queries](#3-filter-queries)
4. [Projection Queries](#4-projection-queries)
5. [Aggregation Queries](#5-aggregation-queries)
6. [Combined Queries](#6-combined-queries)
7. [Time-Series Analytics](#7-time-series-analytics)
8. [CRUD Workflow](#8-crud-workflow)
9. [Persistence & Reload](#9-persistence--reload)
10. [Compression & Encodings](#10-compression--encodings)

---

## 1. Quick Start

### Add to your project

```toml
[dependencies]
tuckdb = "0.1"
```

### First program

```rust
use std::path::PathBuf;
use tuckdb::*;

fn main() {
    let dir = PathBuf::from("/tmp/tuckdb_demo");
    std::fs::create_dir_all(&dir).unwrap();

    // Define schema
    let schema = Schema::new(vec![
        Field::new("city", DataType::Utf8, false),
        Field::new("temp", DataType::Float64, true),
    ]);

    // Create table
    let mut table = Table::create("weather", schema.clone(), dir);

    // Insert data
    let batch = RecordBatch::new(
        schema,
        vec![
            Column::new(schema.fields[0].clone(),
                ColumnData::Utf8(vec!["NYC".into(), "LA".into(), "NYC".into()])),
            Column::new(schema.fields[1].clone(),
                ColumnData::Float64(vec![72.0, 85.0, 68.0])),
        ],
    );
    table.insert_batch(batch);

    // Query: SELECT city, AVG(temp) WHERE temp > 70 GROUP BY city
    let plan = LogicalPlan::scan("weather")
        .filter(col("temp").gt(lit_float(70.0)))
        .aggregate(
            vec![(AggOp::Avg, "temp", "avg_temp")],
            vec!["city".to_string()],
        );

    let mut rs = table.execute(plan);
    while let Some(batch) = rs.next_batch() {
        for row in 0..batch.num_rows {
            let city = match &batch.columns[0].data {
                ColumnData::Utf8(v) => &v[row],
                _ => "",
            };
            let avg = match &batch.columns[1].data {
                ColumnData::Float64(v) => v[row],
                _ => 0.0,
            };
            println!("{}: {:.1}", city, avg);
        }
    }
}
```

**Expected output:**
```
LA: 85.0
NYC: 72.0
```

### Run existing examples

```bash
# Full demo (100K rows)
cargo run --example demo_app

# Basic CRUD operations
cargo run --example basic_operations

# Time-series analytics
cargo run --example time_series

# CRUD workflow with persistence
cargo run --example crud_workflow
```

---

## 2. Basic Operations

### Create table, insert, scan

```rust
use tuckdb::*;

let schema = Schema::new(vec![
    Field::new("id", DataType::Int64, false),
    Field::new("name", DataType::Utf8, false),
    Field::new("score", DataType::Float64, true),
]);

let mut table = Table::create("students", schema.clone(), path);

// Insert first batch
let batch1 = RecordBatch::new(schema.clone(), vec![
    Column::new(field.clone(), ColumnData::Int64(vec![1, 2, 3])),
    Column::new(field.clone(), ColumnData::Utf8(vec!["Alice", "Bob", "Charlie"])),
    Column::new(field.clone(), ColumnData::Float64(vec![85.5, 92.0, 78.3])),
]);
table.insert_batch(batch1);

// Insert second batch
let batch2 = RecordBatch::new(schema.clone(), vec![
    Column::new(field.clone(), ColumnData::Int64(vec![4, 5])),
    Column::new(field.clone(), ColumnData::Utf8(vec!["Diana", "Eve"])),
    Column::new(field.clone(), ColumnData::Float64(vec![95.0, 88.1])),
]);
table.insert_batch(batch2);

println!("Total rows: {}", table.num_rows());

// Scan all
let mut rs = table.scan_all();
while let Some(batch) = rs.next_batch() {
    println!("Batch with {} rows", batch.num_rows);
}
```

**Expected output:**
```
Total rows: 5
Batch with 3 rows
Batch with 2 rows
```

---

## 3. Filter Queries

### WHERE clause with comparisons

```rust
// WHERE score > 80
let plan = LogicalPlan::scan("students")
    .filter(col("score").gt(lit_float(80.0)));
```

**Supported operators:**

| Operator | Example | Description |
|----------|---------|-------------|
| `.eq()` | `col("x").eq(lit_int(5))` | Equal to |
| `.neq()` | `col("x").neq(lit_str("foo"))` | Not equal to |
| `.gt()` | `col("x").gt(lit_float(10.5))` | Greater than |
| `.gte()` | `col("x").gte(lit_int(0))` | Greater than or equal |
| `.lt()` | `col("x").lt(lit_int(100))` | Less than |
| `.lte()` | `col("x").lte(lit_float(50.0))` | Less than or equal |
| `.and()` | `a.and(b)` | Logical AND |
| `.or()` | `a.or(b)` | Logical OR |

### Compound filter

```rust
// WHERE score > 80 AND active = 1
let plan = LogicalPlan::scan("students")
    .filter(
        col("score").gt(lit_float(80.0))
            .and(col("active").eq(lit_int(1)))
    );
```

**Expected behavior:**
- Filters are pushed into the Scan node by the optimizer
- Chunk-level stats (min/max) are used to skip irrelevant chunks before decoding
- Only rows matching the predicate are returned

### Example: Filter output

```rust
let plan = LogicalPlan::scan("students")
    .filter(col("score").gt(lit_float(80.0)));
let mut rs = table.execute(plan);
while let Some(batch) = rs.next_batch() {
    for row in 0..batch.num_rows {
        // process filtered rows only
    }
}
```

**Expected output for data [85.5, 92.0, 78.3, 95.0, 88.1]:**
```
Found 4 rows with score > 80 (Alice, Bob, Diana, Eve)
Charlie (78.3) is excluded
```

---

## 4. Projection Queries

### SELECT specific columns

```rust
// SELECT name, score
let plan = LogicalPlan::scan("students")
    .project(&["name", "score"]);
let mut rs = table.execute(plan);
while let Some(batch) = rs.next_batch() {
    // batch.columns.len() == 2 (only name and score)
}
```

**Important:** The optimizer pushes the projection into the Scan, so only requested columns are decoded from compressed storage. Unneeded columns are never read.

### Combined with filter

```rust
// SELECT name, score WHERE score > 80
let plan = LogicalPlan::scan("students")
    .filter(col("score").gt(lit_float(80.0)))
    .project(&["name", "score"]);
```

**Expected output:**
```
Alice: 85.5
Bob: 92.0
Diana: 95.0
Eve: 88.1
```

---

## 5. Aggregation Queries

### Available aggregate functions

| Function | Description |
|----------|-------------|
| `AggOp::Sum` | Sum of values |
| `AggOp::Count` | Count of non-null values |
| `AggOp::Avg` | Average (mean) |
| `AggOp::Min` | Minimum value |
| `AggOp::Max` | Maximum value |

### Simple aggregation (no GROUP BY)

```rust
let plan = LogicalPlan::scan("students")
    .project(&["score"])
    .aggregate(
        vec![
            (AggOp::Count, "score", "count"),
            (AggOp::Sum, "score", "total"),
            (AggOp::Avg, "score", "avg"),
            (AggOp::Min, "score", "min"),
            (AggOp::Max, "score", "max"),
        ],
        vec![],  // no group by → single row
    );
```

### Aggregation with GROUP BY

```rust
// SELECT active, AVG(score), COUNT(*)
// GROUP BY active
let plan = LogicalPlan::scan("students")
    .project(&["active", "score"])
    .aggregate(
        vec![
            (AggOp::Count, "score", "count"),
            (AggOp::Avg, "score", "avg_score"),
        ],
        vec!["active".to_string()],
    );
```

**Expected output for data with active=1 (Alice, Bob, Eve) and active=0 (Charlie, Diana):**
```
active=0: count=2, avg=86.7
active=1: count=3, avg=88.5
```

---

## 6. Combined Queries

### Filter + Project + Aggregate

```rust
// SELECT active, AVG(score) as avg_score, MAX(score) as max_score
// WHERE score > 80
// GROUP BY active
let plan = LogicalPlan::scan("students")
    .filter(col("score").gt(lit_float(80.0)))
    .project(&["active", "score"])
    .aggregate(
        vec![
            (AggOp::Avg, "score", "avg_score"),
            (AggOp::Max, "score", "max_score"),
        ],
        vec!["active".to_string()],
    );
```

### Composite filter

```rust
// WHERE (score > 80 OR score < 70) AND active = 1
let plan = LogicalPlan::scan("students")
    .filter(
        col("score").gt(lit_float(80.0))
            .or(col("score").lt(lit_float(70.0)))
            .and(col("active").eq(lit_int(1)))
    );
```

---

## 7. Time-Series Analytics

### Schema for metrics data

```rust
let schema = Schema::new(vec![
    Field::new("ts", DataType::Timestamp, false),
    Field::new("server", DataType::Utf8, false),
    Field::new("cpu", DataType::Float64, true),
    Field::new("mem", DataType::Float64, true),
    Field::new("requests", DataType::Int64, true),
]);
```

### Query: High CPU servers

```
// SELECT ts, server, cpu, requests WHERE cpu > 70
```

**Expected output (21 rows from 30 total):**
```
[1700018000] web-01 cpu=75.0% reqs=200
[1700021600] web-01 cpu=80.0% reqs=220
... 
[1700032400] db-01 cpu=115.0% reqs=380
Total: 21 rows
```

### Query: Average metrics per server

```
// SELECT server, AVG(cpu), AVG(mem), SUM(requests) GROUP BY server
```

**Expected output:**
```
web-01: avg_cpu=72.5% avg_mem=69.0% total_reqs=1900
web-02: avg_cpu=82.5% avg_mem=74.0% total_reqs=2400
db-01:  avg_cpu=92.5% avg_mem=79.0% total_reqs=2900
```

### Query: Peak load per server

```
// SELECT server, MAX(requests) as peak_reqs GROUP BY server
```

**Expected output:**
```
web-01 peak requests: 280
web-02 peak requests: 330
db-01 peak requests: 380
```

### Run it

```bash
cargo run --example time_series
```

---

## 8. CRUD Workflow

### CREATE

```rust
let schema = Schema::new(vec![
    Field::new("id", DataType::Int64, false),
    Field::new("product", DataType::Utf8, false),
    Field::new("price", DataType::Float64, true),
    Field::new("stock", DataType::Int64, false),
]);
let mut table = Table::create("inventory", schema, path);
```

### INSERT (Create)

```rust
table.insert_batch(batch);
// Or insert multiple batches
table.insert_batch(batch1);
table.insert_batch(batch2);
```

### READ (Query)

```rust
// Read all
table.scan_all()

// Read with filter
let plan = LogicalPlan::scan("inventory")
    .filter(col("price").gt(lit_float(20.0)));
table.execute(plan)

// Read with aggregation
let plan = LogicalPlan::scan("inventory")
    .aggregate(
        vec![(AggOp::Sum, "stock", "total_stock")],
        vec![],
    );
table.execute(plan)
```

### UPDATE (simulated)

TuckDB is append-only. "Update" means inserting a new version:

```rust
table.insert_batch(updated_batch);
// data_version is automatically incremented
// ResultCache is invalidated on insert
```

### DELETE (simulated)

Use a filter to exclude unwanted rows:

```rust
// "Delete" low-stock items by not selecting them
let plan = LogicalPlan::scan("inventory")
    .filter(col("stock").gte(lit_int(15)));
```

### Full CRUD example output

```bash
cargo run --example crud_workflow
```

```
--- CREATE ---
  Created 'inventory' table with 3 products

--- READ (initial) ---
  All products:
    1. 101 Widget 10.0 100
    2. 102 Gadget 25.0 50
    3. 103 Doohickey 50.0 25

--- INSERT more data ---
  Inserted 2 more products

  All products after insert:
    1. 101 Widget 10.0 100
    2. 102 Gadget 25.0 50
    3. 103 Doohickey 50.0 25
    4. 104 Thingamajig 15.0 10
    5. 105 Whatchamacallit 35.0 5

--- READ with filter (price > 20) ---
  Products with price > $20:
    1. Gadget 25.0 50
    2. Doohickey 50.0 25
    3. Whatchamacallit 35.0 5

--- AGGREGATE (inventory value) ---
  Total products: 5, Total stock: 190
```

---

## 9. Persistence & Reload

### Flush to disk

```rust
table.flush();
// Creates: /path/to/base/<table_name>.tuck
```

### Reload from disk

```rust
let table = Table::open("my_table", base_path);
// Reads .tuck file, decompresses chunks, rebuilds in-memory state
```

### Verify persistence

```bash
cargo run --example basic_operations
```

```
Step 9: Flush to disk and reload
  Flushed to /tmp/tuckdb_scenarios/users.tuck
  Reloaded 5 rows from disk

Step 10: Verify reloaded data (scan all)
  Scanned 5 rows — data integrity verified
```

### Blob file format

The `.tuck` file is a binary format:

```
┌──────┬─────────┬────────────┬────────────┬──────────────────────┐
│MAGIC │ VERSION │  Schema    │ ChunkMeta  │ Compressed data      │
│TCKB  │  u32    │ (ser.)     │ [N entries]│ [chunk1, chunk2, ...]│
└──────┴─────────┴────────────┴────────────┴──────────────────────┘
```

Each chunk stores: column index, chunk index, encoding ID, file offset, compressed size, uncompressed size, and statistics (min, max, null count).

---

## 10. Compression & Encodings

### Int64 — Delta + Varint

| Pattern | Typical ratio |
|---------|---------------|
| Sequential (0,1,2,3...) | 8:1 |
| Close values (100,105,110...) | 4:1 |
| Random | 1:1 (no compression) |

**How it works:** Stores the first value as raw `i64`, then each subsequent value as the delta from the previous, encoded as a zigzag varint. Small deltas use fewer bytes.

### Float64 — XOR (Gorilla-style)

| Pattern | Typical ratio |
|---------|---------------|
| Stable time-series (slowly changing) | 4:1 |
| Alternating values | 2:1 |
| Random | 1:1 |

**How it works:** Stores the first value as raw `f64`. For each subsequent value, XORs with the previous and stores leading zeros count, trailing zeros count, and the meaningful middle bits.

### Utf8 — Dictionary + ZSTD

| Cardinality | Typical ratio |
|-------------|---------------|
| Low (e.g., status codes: "ok","error","warn") | 20:1 |
| Medium (e.g., city names) | 10:1 |
| High (e.g., unique IDs) | 2:1 |

**How it works:** Builds a dictionary of unique strings, stores indices as varints, then compresses the entire payload with ZSTD.

### Chunk-level statistics

Each chunk stores `min`, `max`, and `null_count`. During filtered scans:

```
Filter: score > 80

Chunk 1: min=10, max=50   → SKIP (no possible match)
Chunk 2: min=30, max=100  → PARTIAL (decode and filter rows)
Chunk 3: min=90, max=150  → PASS (all rows match, skip filter eval)
```

This avoids decoding chunks that cannot possibly satisfy the filter.

---

## Expression Reference

### Creating expressions

```rust
// Column reference
col("name")

// Literals
lit_int(42)
lit_float(3.14)
lit_str("hello")

// Comparisons
col("x").eq(lit_int(5))
col("x").neq(lit_int(5))
col("x").gt(lit_int(5))
col("x").gte(lit_int(5))
col("x").lt(lit_int(5))
col("x").lte(lit_int(5))

// Logical
a.and(b)
a.or(b)

// Chained
col("x").gt(lit_int(10)).and(col("y").eq(lit_str("foo")))
```

### Expression evaluation

Expressions are evaluated row-by-row in the filter or projected as needed. The evaluator handles type coercion (e.g., `Int64` vs `Float64` comparisons).

---

## Error Handling

TuckDB uses `panic!` for error cases in the current version:

- Schema mismatch on insert
- Unknown encoding ID on read
- Column not found in filter expression
- Disk I/O errors on flush/open

Planned for Phase 5: proper `Result` types throughout.

---

## Performance Tips

1. **Batch your inserts** — Larger batches compress better (dictionary encoding benefits from more rows per chunk)
2. **Project early** — Use `.project()` to select only needed columns; the optimizer pushes this into storage
3. **Filter early** — Filters are pushed into scans; chunk-level stats skip irrelevant data before decode
4. **Reuse table for cache benefit** — DataCache stores compressed chunks across queries, so repeated scans of hot data avoid disk I/O
5. **Flush periodically** — Persists compressed data; reload is faster than re-inserting

---

## Example: Complete Program

```rust
use std::path::PathBuf;
use tuckdb::*;

fn main() {
    let dir = PathBuf::from("/tmp/tuckdb_guide");
    std::fs::create_dir_all(&dir).unwrap();

    // Schema
    let schema = Schema::new(vec![
        Field::new("timestamp", DataType::Timestamp, false),
        Field::new("sensor", DataType::Utf8, false),
        Field::new("value", DataType::Float64, true),
        Field::new("unit", DataType::Utf8, false),
    ]);

    let mut table = Table::create("sensors", schema.clone(), dir);

    // Ingest 3 batches
    for batch_id in 0..3 {
        let n = 1000;
        let ts: Vec<i64> = (batch_id * n..(batch_id + 1) * n)
            .map(|i| 1700000000i64 + i as i64)
            .collect();
        let sensors = vec!["temp"; n].into_iter().map(String::from).collect();
        let values: Vec<f64> = (batch_id * n..(batch_id + 1) * n)
            .map(|i| 20.0 + (i as f64 * 0.1).sin() * 5.0)
            .collect();
        let units = vec!["celsius"; n].into_iter().map(String::from).collect();

        let batch = RecordBatch::new(
            schema.clone(),
            vec![
                Column::new(schema.fields[0].clone(), ColumnData::Timestamp(ts)),
                Column::new(schema.fields[1].clone(), ColumnData::Utf8(sensors)),
                Column::new(schema.fields[2].clone(), ColumnData::Float64(values)),
                Column::new(schema.fields[3].clone(), ColumnData::Utf8(units)),
            ],
        );
        table.insert_batch(batch);
    }

    println!("Ingested {} rows", table.num_rows());

    // Query: AVG(value), MIN(value), MAX(value) by sensor
    let plan = LogicalPlan::scan("sensors")
        .project(&["sensor", "value"])
        .aggregate(
            vec![
                (AggOp::Avg, "value", "avg_value"),
                (AggOp::Min, "value", "min_value"),
                (AggOp::Max, "value", "max_value"),
            ],
            vec!["sensor".to_string()],
        );

    let mut rs = table.execute(plan);
    while let Some(batch) = rs.next_batch() {
        for row in 0..batch.num_rows {
            let sensor = match &batch.columns[0].data {
                ColumnData::Utf8(v) => &v[row],
                _ => "",
            };
            let avg = match &batch.columns[1].data {
                ColumnData::Float64(v) => v[row],
                _ => 0.0,
            };
            let min = match &batch.columns[2].data {
                ColumnData::Float64(v) => v[row],
                _ => 0.0,
            };
            let max = match &batch.columns[3].data {
                ColumnData::Float64(v) => v[row],
                _ => 0.0,
            };
            println!("{}: avg={:.2}, min={:.2}, max={:.2}", sensor, avg, min, max);
        }
    }

    // Persist
    table.flush();
    println!("Data persisted");

    // Reload
    let reloaded = Table::open("sensors", dir);
    println!("Reloaded {} rows", reloaded.num_rows());
}
```

**Expected output:**
```
Ingested 3000 rows
temp: avg=20.00, min=15.00, max=25.00
Data persisted
Reloaded 3000 rows
```
