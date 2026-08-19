use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

use csv::ReaderBuilder;
use sqlparser::ast::{
    BinaryOperator, Expr as SqlExpr, FunctionArg, FunctionArgExpr, FunctionArguments, SelectItem,
    SetExpr, Statement, Value,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use tuckdb::embedding::{EmbeddingProvider, MockEmbeddingProvider};
use tuckdb::exec::expr::{Expr as TkExpr, col, lit_float, lit_int, lit_str};
use tuckdb::exec::logical_plan::LogicalPlan;
use tuckdb::{Column, ColumnData, DataType, Field, IncrementalEngine, RecordBatch, Schema, Table};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (csv_path, queries) = parse_args();

    println!("========================================================================");
    println!("  TuckDB-AI | Unified SQL + Vector engine");
    println!("  Loads a real CSV into a .tuck table, indexes the same rows into the");
    println!("  vector DB, then routes each query: SIMILARITY -> vector DB,");
    println!("  otherwise -> structured data from the .tuck file.");
    println!("========================================================================");
    println!();

    let (names, rows) = read_csv(&csv_path)?;
    println!(
        "[data]   loaded {} rows x {} columns from {}",
        rows.len(),
        names.len(),
        csv_path.display()
    );

    let schema = infer_schema(&names, &rows);
    let dir = PathBuf::from("/tmp/tuckdb_unified");
    let mut table = Table::create("cpp_questions", schema.clone(), dir.clone());
    table.insert_batch(rows_to_batch(&schema, &rows));
    table.flush();
    drop(table);
    let size = std::fs::metadata(dir.join("cpp_questions.tuck"))?.len();
    println!("[tuck]   persisted cpp_questions.tuck ({} bytes)", size);

    let mut engine = IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()));
    for (i, row) in rows.iter().enumerate() {
        engine.ingest(i as i64, &join(row));
    }
    println!(
        "[vector] indexed {} rows -> {} chunks -> {} vectors (model \"model-v1\")",
        rows.len(),
        engine.store().len(),
        engine.store().len()
    );

    let vec_path = dir.join("cpp_questions.tvec");
    engine.store().save_to(&vec_path)?;
    println!(
        "[vector] vector DB persisted to {} ({} bytes)",
        vec_path.display(),
        std::fs::metadata(&vec_path)?.len()
    );

    let mut engine = IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()));
    *engine.store_mut() = tuckdb::EmbeddingStore::load_from(&vec_path)?;
    println!(
        "[vector] loaded {} vectors from vector DB file {} (no re-embedding needed)",
        engine.store().len(),
        vec_path.display()
    );
    println!();

    let run = |sql: &str| -> Result<(), Box<dyn std::error::Error>> {
        println!("SQL: {}", sql);
        run_query(sql, &names, &rows, &engine, &dir)?;
        println!();
        Ok(())
    };

    if !queries.is_empty() {
        for q in &queries {
            run(q)?;
        }
    } else {
        println!(
            "Type SQL (e.g. WHERE topic = 'cpp', or WHERE SIMILARITY(answer, '...') > 0.3, or WHERE VECTOR_SEARCH(answer, '...', 0.3)); 'exit' / Ctrl-D to quit."
        );
        let stdin = io::stdin();
        let mut lines = stdin.lock().lines();
        loop {
            print!("SQL> ");
            io::stdout().flush()?;
            let line = match lines.next() {
                Some(Ok(l)) => l,
                _ => break,
            };
            let q = line.trim().to_string();
            if q.is_empty() {
                continue;
            }
            if q.eq_ignore_ascii_case("exit") || q.eq_ignore_ascii_case("quit") {
                break;
            }
            run(&q)?;
        }
    }
    Ok(())
}

fn run_query(
    sql: &str,
    names: &[String],
    rows: &[Vec<String>],
    engine: &IncrementalEngine,
    dir: &PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let dialect = GenericDialect {};
    let mut stmts = Parser::parse_sql(&dialect, sql)?;
    let stmt = stmts.remove(0);
    let (select, limit) = match &stmt {
        Statement::Query(q) => match &*q.body {
            SetExpr::Select(s) => (s, q.limit.as_ref()),
            _ => return Err("only SELECT supported".into()),
        },
        _ => return Err("only SELECT supported".into()),
    };
    let limit_n = limit
        .and_then(|e| match e {
            SqlExpr::Value(v) => match &v.value {
                Value::Number(n, _) => n.parse::<usize>().ok(),
                _ => None,
            },
            _ => None,
        })
        .unwrap_or(usize::MAX);

    let proj: Vec<String> = select
        .projection
        .iter()
        .filter_map(|item| match item {
            SelectItem::UnnamedExpr(SqlExpr::Identifier(id)) => Some(id.value.clone()),
            _ => None,
        })
        .collect();

    let is_vector = select
        .selection
        .as_ref()
        .map(contains_vector_operator)
        .unwrap_or(false);

    if is_vector {
        let identified = select
            .selection
            .as_ref()
            .and_then(vector_operator_name)
            .unwrap_or("SIMILARITY(...)");
        let mut p = VecParams::default();
        if let Some(e) = &select.selection {
            parse_vector_params(e, &mut p);
        }
        println!(
            "Route: VECTOR ENGINE (identified by clause: {})",
            identified
        );
        println!(
            "    semantic: column=\"{}\" query=\"{}\" threshold={}",
            p.column, p.query, p.threshold
        );
        if let Some((c, v)) = &p.prefilter {
            println!("    structured pre-filter: {} = '{}'", c, v);
        }

        let qv = MockEmbeddingProvider::default().embed(&p.query);
        let results = engine.store().search(&qv, 64);
        let mut seen = std::collections::HashSet::new();
        let mut hits: Vec<(usize, f64)> = Vec::new();
        for m in results {
            if m.score < p.threshold {
                continue;
            }
            let ri = m.document_id as usize;
            if let Some((c, v)) = &p.prefilter {
                if let Some(ci) = names.iter().position(|n| n == c) {
                    if rows[ri][ci] != *v {
                        continue;
                    }
                }
            }
            if seen.insert(ri) {
                hits.push((ri, m.score));
            }
        }
        hits.truncate(limit_n);
        println!("    results ({} rows):", hits.len());
        for (idx, (ri, score)) in hits.iter().enumerate() {
            println!(
                "      #{} score={:.3} row={}: {}",
                idx + 1,
                score,
                ri,
                format_row(rows, names, *ri, &proj)
            );
        }
    } else {
        let mut plan = build_structured_plan(select, "cpp_questions")?;
        if limit_n != usize::MAX {
            plan = plan.limit(limit_n);
        }
        let mut table = Table::open("cpp_questions", dir.clone());
        println!("Route: STRUCTURED ENGINE (no SIMILARITY) - reads from cpp_questions.tuck");
        let mut rs = table.execute(plan);
        let mut out = Vec::new();
        while let Some(b) = rs.next_batch() {
            for row in 0..b.num_rows {
                let vals: Vec<String> = b
                    .columns
                    .iter()
                    .map(|c| match &c.data {
                        ColumnData::Int64(v) => v[row].to_string(),
                        ColumnData::Float64(v) => format!("{:.1}", v[row]),
                        ColumnData::Utf8(v) => v[row].clone(),
                        ColumnData::Timestamp(v) => v[row].to_string(),
                    })
                    .collect();
                out.push(vals.join(" | "));
            }
        }
        println!("    results ({} rows):", out.len());
        for line in &out {
            println!("      {}", line);
        }
    }
    Ok(())
}

/// Returns true if the expression contains a vector operator clause
/// (SIMILARITY or VECTOR_SEARCH), i.e. the query must be routed to the
/// vector DB.
fn contains_vector_operator(e: &SqlExpr) -> bool {
    match e {
        SqlExpr::Function(f) => is_vector_function(f),
        SqlExpr::Nested(inner) => contains_vector_operator(inner),
        SqlExpr::BinaryOp { left, right, .. } => {
            contains_vector_operator(left) || contains_vector_operator(right)
        }
        _ => false,
    }
}

/// Name of the vector operator clause that identifies the query as a
/// vector query (used for the routing report).
fn vector_operator_name(e: &SqlExpr) -> Option<&'static str> {
    match e {
        SqlExpr::Function(f) => match f.name.to_string().to_uppercase().as_str() {
            "SIMILARITY" => Some("SIMILARITY(...)"),
            "VECTOR_SEARCH" => Some("VECTOR_SEARCH(...)"),
            _ => None,
        },
        SqlExpr::Nested(inner) => vector_operator_name(inner),
        SqlExpr::BinaryOp { left, right, .. } => {
            vector_operator_name(left).or_else(|| vector_operator_name(right))
        }
        _ => None,
    }
}

fn is_vector_function(f: &sqlparser::ast::Function) -> bool {
    let name = f.name.to_string().to_uppercase();
    name == "SIMILARITY" || name == "VECTOR_SEARCH"
}

#[derive(Default)]
struct VecParams {
    column: String,
    query: String,
    threshold: f64,
    prefilter: Option<(String, String)>,
}

fn parse_vector_params(e: &SqlExpr, p: &mut VecParams) {
    match e {
        SqlExpr::Nested(inner) => parse_vector_params(inner, p),
        SqlExpr::Function(f) if is_vector_function(f) => {
            let (c, q, thr) = vector_args(f);
            p.column = c;
            p.query = q;
            if let Some(t) = thr {
                p.threshold = t;
            }
        }
        SqlExpr::BinaryOp {
            left,
            op: BinaryOperator::And,
            right,
        } => {
            parse_vector_params(left, p);
            parse_vector_params(right, p);
        }
        SqlExpr::BinaryOp { left, op, right } => match (left.as_ref(), right.as_ref()) {
            (SqlExpr::Function(f), SqlExpr::Value(v))
                if is_vector_function(f) && *op == BinaryOperator::Gt =>
            {
                let (c, q, _) = vector_args(f);
                p.column = c;
                p.query = q;
                p.threshold = literal_f64(v);
            }
            (SqlExpr::Identifier(id), SqlExpr::Value(v)) if *op == BinaryOperator::Eq => {
                p.prefilter = Some((id.value.clone(), literal_string(v)));
            }
            _ => {}
        },
        _ => {}
    }
}

/// Extract (column, query, optional threshold) from a vector operator.
/// SIMILARITY(col, 'q') has 2 args; VECTOR_SEARCH(col, 'q', threshold) has 3.
fn vector_args(f: &sqlparser::ast::Function) -> (String, String, Option<f64>) {
    if let FunctionArguments::List(list) = &f.args {
        if list.args.len() >= 2 {
            let col = match &list.args[0] {
                FunctionArg::Unnamed(FunctionArgExpr::Expr(SqlExpr::Identifier(id))) => {
                    id.value.clone()
                }
                _ => String::new(),
            };
            let q = match &list.args[1] {
                FunctionArg::Unnamed(FunctionArgExpr::Expr(SqlExpr::Value(v))) => literal_string(v),
                _ => String::new(),
            };
            let thr = if list.args.len() >= 3 {
                match &list.args[2] {
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(SqlExpr::Value(v))) => {
                        Some(literal_f64(v))
                    }
                    _ => None,
                }
            } else {
                None
            };
            return (col, q, thr);
        }
    }
    (String::new(), String::new(), None)
}

fn literal_f64(v: &sqlparser::ast::ValueWithSpan) -> f64 {
    match &v.value {
        Value::Number(n, _) => n.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn literal_string(v: &sqlparser::ast::ValueWithSpan) -> String {
    match &v.value {
        Value::SingleQuotedString(s) => s.clone(),
        _ => String::new(),
    }
}

fn build_structured_plan(
    select: &sqlparser::ast::Select,
    table: &str,
) -> Result<LogicalPlan, String> {
    let mut plan = LogicalPlan::scan(table);
    if let Some(sel) = &select.selection {
        plan = plan.filter(simple_where_to_expr(sel)?);
    }
    let cols: Vec<&str> = select
        .projection
        .iter()
        .filter_map(|item| match item {
            SelectItem::UnnamedExpr(SqlExpr::Identifier(id)) => Some(id.value.as_str()),
            _ => None,
        })
        .collect();
    if !cols.is_empty() {
        plan = plan.project(&cols);
    }
    Ok(plan)
}

fn simple_where_to_expr(e: &SqlExpr) -> Result<TkExpr, String> {
    match e {
        SqlExpr::Identifier(id) => Ok(col(&id.value)),
        SqlExpr::Value(v) => match &v.value {
            Value::Number(n, _) => {
                if let Ok(i) = n.parse::<i64>() {
                    Ok(lit_int(i))
                } else {
                    Ok(lit_float(n.parse::<f64>().map_err(|e| e.to_string())?))
                }
            }
            Value::SingleQuotedString(s) => Ok(lit_str(s)),
            _ => Err("unsupported literal".to_string()),
        },
        SqlExpr::BinaryOp { left, op, right } => {
            let l = simple_where_to_expr(left)?;
            let r = simple_where_to_expr(right)?;
            match op {
                BinaryOperator::Eq => Ok(l.eq(r)),
                BinaryOperator::And => Ok(l.and(r)),
                _ => Err("unsupported operator in structured filter".to_string()),
            }
        }
        SqlExpr::Nested(inner) => simple_where_to_expr(inner),
        _ => Err("unsupported expression".to_string()),
    }
}

fn read_csv(path: &PathBuf) -> Result<(Vec<String>, Vec<Vec<String>>), Box<dyn std::error::Error>> {
    let mut rdr = ReaderBuilder::new().has_headers(true).from_path(path)?;
    let names: Vec<String> = rdr.headers()?.iter().map(|h| h.to_string()).collect();
    let mut rows = Vec::new();
    for rec in rdr.records() {
        let rec = rec?;
        rows.push(rec.iter().map(|f| f.to_string()).collect());
    }
    Ok((names, rows))
}

fn infer_schema(names: &[String], rows: &[Vec<String>]) -> Schema {
    let fields = names
        .iter()
        .enumerate()
        .map(|(ci, name)| {
            let all_i64 = rows.iter().all(|r| r[ci].parse::<i64>().is_ok());
            let all_f64 = rows.iter().all(|r| r[ci].parse::<f64>().is_ok());
            let dt = if all_i64 {
                DataType::Int64
            } else if all_f64 {
                DataType::Float64
            } else {
                DataType::Utf8
            };
            Field::new(name, dt, true)
        })
        .collect();
    Schema::new(fields)
}

fn rows_to_batch(schema: &Schema, rows: &[Vec<String>]) -> RecordBatch {
    let mut columns = Vec::new();
    for (ci, field) in schema.fields.iter().enumerate() {
        let data = match field.data_type {
            DataType::Int64 => {
                ColumnData::Int64(rows.iter().map(|r| r[ci].parse().unwrap_or(0)).collect())
            }
            DataType::Float64 => {
                ColumnData::Float64(rows.iter().map(|r| r[ci].parse().unwrap_or(0.0)).collect())
            }
            DataType::Utf8 => ColumnData::Utf8(rows.iter().map(|r| r[ci].clone()).collect()),
            DataType::Timestamp => ColumnData::Timestamp(Vec::new()),
        };
        columns.push(Column::new(field.clone(), data));
    }
    RecordBatch::new(schema.clone(), columns)
}

fn join(row: &[String]) -> String {
    row.iter()
        .map(|v| v.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn format_row(rows: &[Vec<String>], names: &[String], row_idx: usize, cols: &[String]) -> String {
    if cols.is_empty() {
        return rows[row_idx].join(" | ");
    }
    cols.iter()
        .map(|c| {
            names
                .iter()
                .position(|n| n == c)
                .map(|i| rows[row_idx][i].clone())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn parse_args() -> (PathBuf, Vec<String>) {
    let args: Vec<String> = std::env::args().collect();
    let mut csv_path = PathBuf::from("data/cpp_questions.csv");
    let mut queries: Vec<String> = Vec::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--csv" => {
                i += 1;
                csv_path = PathBuf::from(&args[i]);
            }
            "--query" => {
                i += 1;
                queries.push(args[i].clone());
            }
            other => {
                eprintln!("unknown argument: {}", other);
                std::process::exit(1);
            }
        }
        i += 1;
    }
    (csv_path, queries)
}
