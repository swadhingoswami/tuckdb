use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use sqlparser::ast::{
    BinaryOperator, Expr as SqlExpr, FunctionArgExpr, SelectItem, SetExpr, Statement, Value,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use tuckdb::embedding::dedup::SemanticDedup;
use tuckdb::embedding::{EmbeddingProvider, MockEmbeddingProvider};
use tuckdb::exec::expr::{Expr as TkExpr, col, lit_float, lit_int, lit_str};
use tuckdb::exec::logical_plan::LogicalPlan;
use tuckdb::lifecycle::ContentDedup;
use tuckdb::{Column, ColumnData, DataType, Field, IncrementalEngine, RecordBatch, Schema, Table};

const TOPICS: [&str; 10] = [
    "Resource Acquisition Is Initialization binds resource lifetime to object lifetime.",
    "A move constructor transfers resources from another object.",
    "A virtual function enables runtime polymorphism.",
    "A mutex provides mutual exclusion for concurrent access.",
    "Smart pointers transfer ownership without copying.",
    "A lambda captures variables from the enclosing scope.",
    "RAII acquires resources in the constructor and releases them in the destructor.",
    "Move semantics avoid unnecessary copies of large objects.",
    "Templates enable compile-time polymorphism.",
    "Destructors run automatically when an object goes out of scope.",
];

const CHUNKS_PER_DOC: usize = 5;

fn main() {
    let (scale, changed, deleted) = parse_args();
    assert!(
        changed + deleted <= scale,
        "changed + deleted must not exceed scale"
    );

    println!("========================================================================");
    println!("  TuckDB-AI  |  Incremental AI Data Engine");
    println!("  \"Process what changed. Leave everything else untouched.\"");
    println!("========================================================================");
    println!();
    println!("  Problem: a small change in AI data can trigger expensive reprocessing");
    println!("           of large datasets. This engine detects exactly what changed");
    println!("           and updates only the affected data.");
    println!();

    let mut engine = IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()));

    // ------------------------------------------------------------------ [1]
    println!("[1] Build the AI index (mock model \"model-v1\", 128 dims)");
    let t = Instant::now();
    for id in 0..scale {
        engine.ingest(id as i64, &doc_content(id));
    }
    let build_ms = t.elapsed().as_millis();
    let total = engine.store().len();
    println!(
        "    {} documents x {} chunks = {} vectors indexed in {} ms",
        scale, CHUNKS_PER_DOC, total, build_ms
    );
    let t = Instant::now();
    let mut n = 0usize;
    for id in 0..scale.min(1000) {
        for chunk in engine.chunks().get(id as i64).unwrap() {
            black_box(provider().embed(&chunk.content));
            n += 1;
        }
    }
    let per_chunk_us = t.elapsed().as_micros() as f64 / n.max(1) as f64;
    println!(
        "    embedding throughput ~ {:.0} us/chunk (used to estimate the naive baseline)",
        per_chunk_us
    );
    println!();

    // ------------------------------------------------------------------ [2]
    println!("[2] Semantic query (brute-force cosine over stored vectors)");
    let qv = provider().embed("How does C++ transfer ownership without copying?");
    let top = engine.store().search(&qv, 3);
    for (i, m) in top.iter().enumerate() {
        let chunk = &engine.chunks().get(m.document_id).unwrap()[m.chunk_id as usize];
        println!(
            "    #{} score={:.3} (doc {}, chunk {})",
            i + 1,
            m.score,
            m.document_id,
            m.chunk_id
        );
        println!("       \"{}\"", chunk.content);
    }
    println!();

    // ------------------------------------------------------------------ [3]
    println!(
        "[3] UPDATE {} documents - incremental: re-embed ONLY changed chunks",
        changed
    );
    let t = Instant::now();
    let mut generated = 0usize;
    for id in 0..changed {
        let r = engine.update(id as i64, &updated_content(id)).unwrap();
        generated += r.embeddings_generated;
    }
    let update_ms = t.elapsed().as_millis();
    let avoided = (1.0 - generated as f64 / total as f64) * 100.0;
    let naive_est = (per_chunk_us * total as f64 / 1000.0) as u64;
    println!("    Dataset:            {:>8} chunks", total);
    println!("    Changed:            {:>8}", changed);
    println!("    Processed:          {:>8}", generated);
    println!("    Skipped:            {:>8}", total - generated);
    println!("    Work avoided:       {:>7.1}%", avoided);
    println!(
        "    Elapsed:            {:>7} ms  (naive full re-embed would take ~{} ms)",
        update_ms, naive_est
    );
    assert_eq!(generated, changed);
    println!();

    // ------------------------------------------------------------------ [4]
    println!(
        "[4] DELETE {} documents - clean only their derived data",
        deleted
    );
    let t = Instant::now();
    let mut affected = 0usize;
    for id in changed..changed + deleted {
        let r = engine.delete(id as i64).unwrap();
        affected += r.affected_vectors;
    }
    let del_ms = t.elapsed().as_millis();
    let remaining = engine.store().len();
    println!("    Affected vectors:   {:>8}", affected);
    println!(
        "    Vectors remaining:  {:>8} (unrelated untouched)",
        remaining
    );
    println!(
        "    Elapsed:            {:>7} ms  (cleanup is O(chunks/doc), no full scan)",
        del_ms
    );
    assert_eq!(affected, deleted * CHUNKS_PER_DOC);
    assert_eq!(remaining, total - affected);
    println!();

    // ------------------------------------------------------------------ [5]
    println!("[5] Exact deduplication (byte-identical content, by content hash)");
    let mut dedup = ContentDedup::new();
    for i in 0..100 {
        dedup.insert(&[&format!("topic_{}", i % 90)]);
    }
    let dr = dedup.report();
    println!(
        "    100 inputs -> {} unique contents, {} duplicates detected, {} duplicates avoided",
        dr.unique_contents, dr.duplicates_detected, dr.processing_avoided
    );
    println!();

    // ------------------------------------------------------------------ [6]
    println!("[6] Semantic deduplication (embedding candidates, report only)");
    let mut sem = IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()));
    sem.ingest(1, "What is RAII?");
    sem.ingest(2, "Explain Resource Acquisition Is Initialization.");
    sem.ingest(3, "RAII binds resource lifetime to object lifetime.");
    sem.ingest(
        4,
        "Resource Acquisition Is Initialization binds lifetime to object lifetime.",
    );
    sem.ingest(5, "What is a virtual function?");
    let report = SemanticDedup::new(0.65).detect(sem.store());
    println!("    candidates found: {}", report.matches.len());
    for m in &report.matches {
        println!(
            "      doc {} <-> doc {}  similarity = {:.2}",
            m.document_id, m.other_document_id, m.similarity
        );
    }
    println!("    nothing deleted automatically");
    println!();

    // ------------------------------------------------------------------ [7]
    println!("[7] Embedding model versioning: migrate model-v1 -> model-v2");
    let v2 = Arc::new(MockEmbeddingProvider::new("model-v2", 128));
    let t = Instant::now();
    let mr = engine.migrate_model(v2.clone());
    let mig_ms = t.elapsed().as_millis();
    println!(
        "    migrated {} vectors in {} ms, skipped {}",
        mr.migrated, mig_ms, mr.skipped
    );
    let mr2 = engine.migrate_model(v2);
    println!(
        "    migrate again: migrated {} skipped {} (idempotent, no blind rebuild)",
        mr2.migrated, mr2.skipped
    );
    assert_eq!(mr.migrated, remaining);
    assert_eq!(mr2.skipped, remaining);
    println!();

    // ------------------------------------------------------------------ [8]
    println!("[8] SQL + vector query (hybrid: structured AND semantic in one query)");
    let mut sql_table = build_sql_table();
    let sql = "SELECT id, question FROM cpp_questions \
               WHERE topic = 'memory' \
                 AND SIMILARITY(answer, 'How does C++ transfer ownership?') > 0.3 \
               LIMIT 5";
    println!("    SQL: {}", sql);
    let mut rs = sql_table.execute(build_plan(sql, &sql_table).unwrap());
    while let Some(batch) = rs.next_batch() {
        for row in 0..batch.num_rows {
            let id = match &batch.columns[0].data {
                ColumnData::Int64(v) => v[row],
                _ => 0,
            };
            let question = match &batch.columns[1].data {
                ColumnData::Utf8(v) => &v[row],
                _ => "",
            };
            println!("    -> id={} question=\"{}\"", id, question);
        }
    }
    println!();

    // ------------------------------------------------------------------ [9]
    println!("[9] Evidence summary");
    println!("    - Engine correctness: unit + integration tests in the repo (cargo test).");
    println!(
        "    - Incremental update: {} of {} chunks reprocessed ({:.1}% work avoided).",
        generated, total, avoided
    );
    println!(
        "    - Delete cleanup: {} vectors removed directly, {} untouched.",
        affected, remaining
    );
    println!(
        "    - Naive baseline would have re-embedded {} chunks ({} ms estimated).",
        total, naive_est
    );
    println!("    - Scaling benchmark: cargo run --release --example benchmark_report");
    println!("      (writes target/benchmark_report.md)");
    println!();
    println!("All checks passed.");
}

fn provider() -> MockEmbeddingProvider {
    MockEmbeddingProvider::default()
}

fn parse_args() -> (usize, usize, usize) {
    let args: Vec<String> = std::env::args().collect();
    let mut scale = 10_000usize;
    let mut changed = 1_000usize;
    let mut deleted = 100usize;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--scale" => {
                i += 1;
                scale = args[i].parse().expect("--scale expects a number");
            }
            "--changed" => {
                i += 1;
                changed = args[i].parse().expect("--changed expects a number");
            }
            "--deleted" => {
                i += 1;
                deleted = args[i].parse().expect("--deleted expects a number");
            }
            other => {
                eprintln!("unknown argument: {}", other);
                std::process::exit(1);
            }
        }
        i += 1;
    }
    (scale, changed, deleted)
}

fn doc_content(id: usize) -> String {
    (0..CHUNKS_PER_DOC)
        .map(|c| TOPICS[(id + c) % TOPICS.len()])
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn updated_content(id: usize) -> String {
    let mut parts: Vec<String> = (0..CHUNKS_PER_DOC)
        .map(|c| TOPICS[(id + c) % TOPICS.len()].to_string())
        .collect();
    parts[id % CHUNKS_PER_DOC] = format!("{} REVISED", parts[id % CHUNKS_PER_DOC]);
    parts.join("\n\n")
}

// ---------------------------------------------------------------------------
// SQL + vector section (compact converter, same as the CLI/C API path)
// ---------------------------------------------------------------------------

fn build_sql_table() -> Table {
    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("question", DataType::Utf8, false),
        Field::new("answer", DataType::Utf8, false),
        Field::new("topic", DataType::Utf8, false),
    ]);
    let mut table = Table::create(
        "cpp_questions",
        schema.clone(),
        PathBuf::from("/tmp/tuckdb_showcase"),
    );
    let batch = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(
                schema.fields[0].clone(),
                ColumnData::Int64(vec![1, 2, 3, 4, 5]),
            ),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Utf8(vec![
                    "What is RAII?".into(),
                    "How is ownership moved without copying?".into(),
                    "What is a move constructor?".into(),
                    "What is a virtual function?".into(),
                    "What is a mutex?".into(),
                ]),
            ),
            Column::new(
                schema.fields[2].clone(),
                ColumnData::Utf8(vec![
                    "RAII binds resource lifetime to object lifetime.".into(),
                    "Move semantics transfer ownership of resources without copying.".into(),
                    "A move constructor transfers resources from another object.".into(),
                    "A virtual function enables runtime polymorphism.".into(),
                    "A mutex provides mutual exclusion for concurrent access.".into(),
                ]),
            ),
            Column::new(
                schema.fields[3].clone(),
                ColumnData::Utf8(vec![
                    "memory".into(),
                    "memory".into(),
                    "cpp".into(),
                    "cpp".into(),
                    "concurrency".into(),
                ]),
            ),
        ],
    );
    table.insert_batch(batch);
    table
}

fn build_plan(sql: &str, table: &Table) -> Result<LogicalPlan, Box<dyn std::error::Error>> {
    let dialect = GenericDialect {};
    let mut stmts =
        Parser::parse_sql(&dialect, sql).map_err(|e| format!("SQL parse error: {}", e))?;
    if stmts.len() != 1 {
        return Err("expected a single SQL statement".into());
    }
    let stmt = stmts.remove(0);
    let (select, limit) = match &stmt {
        Statement::Query(q) => match &*q.body {
            SetExpr::Select(s) => (s, q.limit.as_ref()),
            _ => return Err("only SELECT supported".into()),
        },
        _ => return Err("only SELECT supported".into()),
    };

    let mut plan = LogicalPlan::scan(table.name());
    if let Some(selection) = &select.selection {
        plan = plan.filter(sql_to_tk_expr(selection)?);
    }
    let cols: Vec<&str> = select
        .projection
        .iter()
        .filter_map(|item| match item {
            SelectItem::UnnamedExpr(SqlExpr::Identifier(ident)) => Some(ident.value.as_str()),
            _ => None,
        })
        .collect();
    if !cols.is_empty() {
        plan = plan.project(&cols);
    }
    if let Some(SqlExpr::Value(vws)) = limit
        && let Value::Number(n, _) = &vws.value
        && let Ok(n) = n.parse::<usize>()
    {
        plan = plan.limit(n);
    }
    Ok(plan)
}

fn sql_to_tk_expr(sql: &SqlExpr) -> Result<TkExpr, Box<dyn std::error::Error>> {
    match sql {
        SqlExpr::Identifier(ident) => Ok(col(&ident.value)),
        SqlExpr::Value(vws) => match &vws.value {
            Value::Number(n, _) => {
                if let Ok(i) = n.parse::<i64>() {
                    Ok(lit_int(i))
                } else {
                    Ok(lit_float(
                        n.parse::<f64>().map_err(|e| format!("Bad number: {}", e))?,
                    ))
                }
            }
            Value::SingleQuotedString(s) => Ok(lit_str(s)),
            _ => Err(format!("Unsupported value: {}", sql).into()),
        },
        SqlExpr::BinaryOp { left, op, right } => {
            let l = sql_to_tk_expr(left)?;
            let r = sql_to_tk_expr(right)?;
            match op {
                BinaryOperator::Eq => Ok(l.eq(r)),
                BinaryOperator::NotEq => Ok(l.neq(r)),
                BinaryOperator::Gt => Ok(l.gt(r)),
                BinaryOperator::GtEq => Ok(l.gte(r)),
                BinaryOperator::Lt => Ok(l.lt(r)),
                BinaryOperator::LtEq => Ok(l.lte(r)),
                BinaryOperator::And => Ok(l.and(r)),
                BinaryOperator::Or => Ok(l.or(r)),
                _ => Err(format!("Unsupported binary operator: {:?}", op).into()),
            }
        }
        SqlExpr::Nested(e) => sql_to_tk_expr(e),
        SqlExpr::Function(f) => {
            let name = f.name.to_string().to_uppercase();
            if name != "SIMILARITY" {
                return Err(format!("Unsupported function: {}", name).into());
            }
            let args = match &f.args {
                sqlparser::ast::FunctionArguments::List(list) => &list.args,
                _ => return Err("SIMILARITY requires two arguments".into()),
            };
            if args.len() != 2 {
                return Err("SIMILARITY requires (column, 'query')".into());
            }
            let col_arg = match &args[0] {
                sqlparser::ast::FunctionArg::Unnamed(e) => e,
                _ => return Err("SIMILARITY first argument must be a column".into()),
            };
            let query_arg = match &args[1] {
                sqlparser::ast::FunctionArg::Unnamed(e) => e,
                _ => return Err("SIMILARITY second argument must be a string literal".into()),
            };
            match (col_arg, query_arg) {
                (
                    FunctionArgExpr::Expr(SqlExpr::Identifier(ident)),
                    FunctionArgExpr::Expr(SqlExpr::Value(vws)),
                ) => match &vws.value {
                    Value::SingleQuotedString(s) => Ok(TkExpr::Similarity {
                        column: ident.value.clone(),
                        query: s.clone(),
                    }),
                    _ => Err("SIMILARITY query must be a string literal".into()),
                },
                _ => Err("SIMILARITY requires (column, 'query')".into()),
            }
        }
        _ => Err(format!("Unsupported SQL expression: {}", sql).into()),
    }
}
