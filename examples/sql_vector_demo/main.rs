use sqlparser::ast::{
    BinaryOperator, Expr as SqlExpr, FunctionArgExpr, SelectItem, SetExpr, Statement, Value,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use std::error::Error;
use std::path::PathBuf;
use tuckdb::exec::expr::{Expr as TkExpr, col, lit_float, lit_int, lit_str};
use tuckdb::exec::logical_plan::LogicalPlan;
use tuckdb::{Column, ColumnData, DataType, Field, RecordBatch, Schema, Table};

fn main() -> Result<(), Box<dyn Error>> {
    println!("=== TuckDB-AI: Phase 6 — SQL + Vector Query Integration ===");
    println!();

    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("question", DataType::Utf8, false),
        Field::new("answer", DataType::Utf8, false),
        Field::new("topic", DataType::Utf8, false),
    ]);
    let mut table = Table::create(
        "cpp_questions",
        schema.clone(),
        PathBuf::from("/tmp/tuckdb_sql"),
    );
    let batch = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(
                schema.fields[0].clone(),
                ColumnData::Int64(vec![1, 2, 3, 4]),
            ),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Utf8(vec![
                    "What is RAII?".into(),
                    "What is a move constructor?".into(),
                    "What is a virtual function?".into(),
                    "What is a mutex?".into(),
                ]),
            ),
            Column::new(
                schema.fields[2].clone(),
                ColumnData::Utf8(vec![
                    "RAII binds resource lifetime to object lifetime.".into(),
                    "A move constructor transfers resources from another object.".into(),
                    "A virtual function enables runtime polymorphism.".into(),
                    "A mutex provides mutual exclusion for concurrent access.".into(),
                ]),
            ),
            Column::new(
                schema.fields[3].clone(),
                ColumnData::Utf8(vec![
                    "memory".into(),
                    "cpp".into(),
                    "cpp".into(),
                    "concurrency".into(),
                ]),
            ),
        ],
    );
    table.insert_batch(batch);

    let sql = "SELECT id, question, answer FROM cpp_questions \
               WHERE SIMILARITY(answer, 'How does C++ transfer ownership without copying?') > 0.4";
    println!("SQL: {}", sql);
    println!();

    let plan = build_plan(sql, &table)?;
    println!("Planner: SIMILARITY(...) routed to the vector engine; predicate pushed into scan.");
    println!();

    let mut rs = table.execute(plan);
    let mut rows = 0;
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
            let answer = match &batch.columns[2].data {
                ColumnData::Utf8(v) => &v[row],
                _ => "",
            };
            println!("id={} question=\"{}\"", id, question);
            println!("    answer=\"{}\"", answer);
            rows += 1;
        }
    }
    println!();
    println!("{} row(s) returned.", rows);
    assert_eq!(rows, 1, "expected only the move-constructor row");
    Ok(())
}

fn build_plan(sql: &str, table: &Table) -> Result<LogicalPlan, Box<dyn Error>> {
    let dialect = GenericDialect {};
    let mut stmts =
        Parser::parse_sql(&dialect, sql).map_err(|e| format!("SQL parse error: {}", e))?;
    if stmts.len() != 1 {
        return Err("expected a single SQL statement".into());
    }
    let stmt = stmts.remove(0);
    let select = match &stmt {
        Statement::Query(q) => match &*q.body {
            SetExpr::Select(s) => s,
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
    Ok(plan)
}

fn sql_to_tk_expr(sql: &SqlExpr) -> Result<TkExpr, Box<dyn Error>> {
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
