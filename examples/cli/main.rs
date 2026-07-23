use std::path::PathBuf;
use std::fs;
use clap::{Parser, Subcommand};
use csv::ReaderBuilder;
use sqlparser::ast::{
    BinaryOperator, Expr as SqlExpr, FunctionArg, FunctionArgExpr, GroupByExpr, SelectItem,
    SetExpr, Statement, TableFactor, Value,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser as SqlParser;
use tuckdb::exec::expr::{col, lit_float, lit_int, lit_str, Expr as TkExpr};
use tuckdb::exec::logical_plan::{AggOp, LogicalPlan};
use tuckdb::*;

#[derive(Parser)]
#[command(name = "tuckdb", about = "TuckDB CLI tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Load {
        csv_path: String,
        #[arg(long = "table")]
        table: String,
        #[arg(long = "dir", default_value = "/tmp/tuckdb")]
        dir: String,
    },
    Query {
        query: String,
        #[arg(long = "dir", default_value = "/tmp/tuckdb")]
        dir: String,
    },
    Scan {
        table: String,
        #[arg(long = "dir", default_value = "/tmp/tuckdb")]
        dir: String,
    },
    Info {
        table: String,
        #[arg(long = "dir", default_value = "/tmp/tuckdb")]
        dir: String,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Load { csv_path, table, dir } => cmd_load(&csv_path, &table, &dir),
        Commands::Query { query, dir } => cmd_query(&query, &dir),
        Commands::Scan { table, dir } => cmd_scan(&table, &dir),
        Commands::Info { table, dir } => cmd_info(&table, &dir),
    };
    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn cmd_load(csv_path: &str, table_name: &str, dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut rdr = ReaderBuilder::new().has_headers(true).from_path(csv_path)?;
    let headers: Vec<String> = rdr.headers()?.iter().map(|h| h.to_string()).collect();
    let num_cols = headers.len();

    let mut all_rows: Vec<Vec<String>> = Vec::new();
    for result in rdr.records() {
        let record = result?;
        let row: Vec<String> = record.iter().map(|f| f.to_string()).collect();
        if row.len() == num_cols {
            all_rows.push(row);
        }
    }

    let sample_size = all_rows.len().min(100);
    let mut data_types: Vec<DataType> = Vec::with_capacity(num_cols);
    for col_idx in 0..num_cols {
        let mut all_i64 = true;
        let mut all_f64 = true;
        for row_idx in 0..sample_size {
            let val = &all_rows[row_idx][col_idx];
            if val.is_empty() {
                continue;
            }
            if val.parse::<i64>().is_err() {
                all_i64 = false;
            }
            if val.parse::<f64>().is_err() {
                all_f64 = false;
            }
        }
        if all_i64 {
            data_types.push(DataType::Int64);
        } else if all_f64 {
            data_types.push(DataType::Float64);
        } else {
            data_types.push(DataType::Utf8);
        }
    }

    let fields: Vec<Field> = headers
        .iter()
        .zip(data_types.iter())
        .map(|(name, dt)| Field::new(name, dt.clone(), true))
        .collect();
    let schema = Schema::new(fields);

    let dir_path = PathBuf::from(dir);
    fs::create_dir_all(&dir_path)?;
    let mut table = Table::create(table_name, schema.clone(), dir_path);

    for chunk in all_rows.chunks(1000) {
        let mut columns = Vec::with_capacity(num_cols);
        for (col_idx, field) in schema.fields.iter().enumerate() {
            let data = match field.data_type {
                DataType::Int64 => {
                    let vals: Vec<i64> = chunk
                        .iter()
                        .map(|row| row[col_idx].parse::<i64>().unwrap_or(0))
                        .collect();
                    ColumnData::Int64(vals)
                }
                DataType::Float64 => {
                    let vals: Vec<f64> = chunk
                        .iter()
                        .map(|row| row[col_idx].parse::<f64>().unwrap_or(0.0))
                        .collect();
                    ColumnData::Float64(vals)
                }
                DataType::Utf8 => {
                    let vals: Vec<String> = chunk.iter().map(|row| row[col_idx].clone()).collect();
                    ColumnData::Utf8(vals)
                }
                DataType::Timestamp => {
                    let vals: Vec<i64> = chunk
                        .iter()
                        .map(|row| row[col_idx].parse::<i64>().unwrap_or(0))
                        .collect();
                    ColumnData::Timestamp(vals)
                }
            };
            columns.push(Column::new(field.clone(), data));
        }
        let batch = RecordBatch::new(schema.clone(), columns);
        table.insert_batch(batch);
    }

    println!("Loaded {} rows into '{}'", all_rows.len(), table_name);
    table.flush();
    println!("Flushed to disk");
    Ok(())
}

fn cmd_query(query: &str, dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    let dir_path = PathBuf::from(dir);

    let dialect = GenericDialect {};
    let mut statements = SqlParser::parse_sql(&dialect, query)
        .map_err(|e| format!("SQL parse error: {}", e))?;
    if statements.is_empty() {
        return Err("No SQL statements found".into());
    }
    let statement = statements.remove(0);

    let select = match &statement {
        Statement::Query(q) => match &*q.body {
            SetExpr::Select(s) => s,
            _ => return Err("Only SELECT queries are supported".into()),
        },
        _ => return Err("Only SELECT queries are supported".into()),
    };

    let table_name = match select.from.first() {
        Some(tj) => match &tj.relation {
            TableFactor::Table { name, .. } => name.0[0].as_ident().unwrap().value.clone(),
            _ => return Err("Only simple table references are supported".into()),
        },
        None => return Err("Query must have a FROM clause".into()),
    };

    let mut table = Table::open(&table_name, dir_path.clone());

    let plan = build_plan(select, &table_name, table.schema())?;
    let mut rs = table.execute(plan);
    print_result_set(&mut rs);
    Ok(())
}

fn cmd_scan(table_name: &str, dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    let dir_path = PathBuf::from(dir);
    if !dir_path.join(format!("{}.tuck", table_name)).exists() {
        return Err(format!("Table '{}' not found in {}", table_name, dir).into());
    }
    let table = Table::open(table_name, dir_path);
    let mut rs = table.scan_all();
    print_result_set(&mut rs);
    Ok(())
}

fn cmd_info(table_name: &str, dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    let dir_path = PathBuf::from(dir);
    let file_path = dir_path.join(format!("{}.tuck", table_name));
    if !file_path.exists() {
        return Err(format!("Table '{}' not found in {}", table_name, dir).into());
    }

    let metadata = fs::metadata(&file_path)?;
    let file_size = metadata.len();

    let table = Table::open(table_name, dir_path);

    println!("Table: {}", table_name);
    println!("Columns:");
    for field in table.schema().fields.iter() {
        println!("  {}: {:?}", field.name, field.data_type);
    }
    println!("Row count: {}", table.num_rows());
    println!("Data version: {}", table.data_version());
    println!("File size: {} bytes", file_size);
    Ok(())
}

fn build_plan(select: &sqlparser::ast::Select, table_name: &str, schema: &Schema) -> Result<LogicalPlan, Box<dyn std::error::Error>> {
    let mut plan = LogicalPlan::scan(table_name);

    if let Some(ref selection) = select.selection {
        let filter_expr = sql_to_tk_expr(selection)?;
        plan = plan.filter(filter_expr);
    }

    let has_aggregates = select.projection.iter().any(|item| {
        let expr = match item {
            SelectItem::UnnamedExpr(e) => e,
            SelectItem::ExprWithAlias { expr, .. } => expr,
            _ => return false,
        };
        is_aggregate_call(expr)
    });

    let has_group_by = matches!(&select.group_by, GroupByExpr::Expressions(exprs, _) if !exprs.is_empty());

    if has_aggregates || has_group_by {
        let group_by_cols = match &select.group_by {
            GroupByExpr::Expressions(exprs, _) => {
                let mut cols = Vec::new();
                for e in exprs {
                    if let SqlExpr::Identifier(ident) = e {
                        cols.push(ident.value.clone());
                    } else {
                        return Err(format!("GROUP BY expression not supported: {}", e).into());
                    }
                }
                cols
            }
            _ => Vec::new(),
        };

        let first_col = schema.fields.first().map(|f| f.name.clone()).unwrap_or_default();

        let mut aggs: Vec<(AggOp, String, String)> = Vec::new();
        let need_final_project = project_aggregate_items(&select.projection, &group_by_cols, &mut aggs, &first_col)?;

        let all_ref_cols = collect_agg_referenced_columns(&select.projection, &group_by_cols, &aggs);

        if !all_ref_cols.is_empty() {
            let proj: Vec<&str> = all_ref_cols.iter().map(|s| s.as_str()).collect();
            plan = plan.project(&proj);
        }

        let agg_refs: Vec<(AggOp, &str, &str)> = aggs.iter().map(|(op, col, name)| (op.clone(), col.as_str(), name.as_str())).collect();
        plan = plan.aggregate(agg_refs, group_by_cols.clone());

        if need_final_project {
            let output_cols = extract_output_cols(&select.projection);
            if !output_cols.is_empty() {
                let proj: Vec<&str> = output_cols.iter().map(|s| s.as_str()).collect();
                plan = plan.project(&proj);
            }
        }
    } else {
        let has_wildcard = select.projection.iter().any(|item| matches!(item, SelectItem::Wildcard(_)));
        if !has_wildcard {
            let cols: Vec<&str> = select
                .projection
                .iter()
                .filter_map(|item| match item {
                    SelectItem::UnnamedExpr(SqlExpr::Identifier(ident)) => Some(ident.value.as_str()),
                    SelectItem::ExprWithAlias { expr: SqlExpr::Identifier(ident), .. } => {
                        Some(ident.value.as_str())
                    }
                    _ => None,
                })
                .collect();
            if !cols.is_empty() {
                plan = plan.project(&cols);
            }
        }
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
                    Ok(lit_float(n.parse::<f64>().map_err(|e| format!("Bad number: {}", e))?))
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
        _ => Err(format!("Unsupported SQL expression: {}", sql).into()),
    }
}

fn is_aggregate_call(expr: &SqlExpr) -> bool {
    match expr {
        SqlExpr::Function(f) => {
            let name = f.name.to_string().to_uppercase();
            matches!(name.as_str(), "COUNT" | "SUM" | "AVG" | "MIN" | "MAX")
        }
        SqlExpr::Nested(e) => is_aggregate_call(e),
        _ => false,
    }
}

fn agg_op_from_name(name: &str) -> Option<AggOp> {
    match name.to_uppercase().as_str() {
        "COUNT" => Some(AggOp::Count),
        "SUM" => Some(AggOp::Sum),
        "AVG" => Some(AggOp::Avg),
        "MIN" => Some(AggOp::Min),
        "MAX" => Some(AggOp::Max),
        _ => None,
    }
}

fn get_function_arg_expr(arg: &FunctionArg) -> Option<&FunctionArgExpr> {
    match arg {
        FunctionArg::Unnamed(e) => Some(e),
        _ => None,
    }
}

fn project_aggregate_items(
    items: &[SelectItem],
    group_by_cols: &[String],
    aggs: &mut Vec<(AggOp, String, String)>,
    first_col: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut output_order: Vec<String> = Vec::new();

    for item in items {
        match item {
            SelectItem::Wildcard(_) => {
                return Err("Wildcard SELECT with aggregation is not supported".into());
            }
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                if let SqlExpr::Function(f) = expr {
                    let func_name = f.name.to_string();
                    if let Some(op) = agg_op_from_name(&func_name) {
                        let (arg_name, is_wildcard) = get_agg_input_col(&f.args, first_col)?;
                        let out_name = match item {
                            SelectItem::ExprWithAlias { alias, .. } => alias.value.clone(),
                            _ => if is_wildcard {
                                format!("{}(*)", func_name.to_uppercase())
                            } else {
                                format!("{}({})", func_name.to_uppercase(), arg_name)
                            },
                        };
                        aggs.push((op, arg_name, out_name.clone()));
                        output_order.push(out_name);
                    }
                } else if let SqlExpr::Identifier(ident) = expr {
                    let col_name = ident.value.clone();
                    if !group_by_cols.contains(&col_name) {
                        return Err(format!(
                            "Column '{}' must appear in GROUP BY or be used in an aggregate function",
                            col_name
                        ).into());
                    }
                    output_order.push(col_name);
                }
            }
            _ => {}
        }
    }

    let default_order: Vec<String> = group_by_cols
        .iter()
        .cloned()
        .chain(aggs.iter().map(|(_, _, n)| n.clone()))
        .collect();

    Ok(output_order != default_order)
}

fn get_agg_input_col(args: &sqlparser::ast::FunctionArguments, first_col: &str) -> Result<(String, bool), Box<dyn std::error::Error>> {
    match args {
        sqlparser::ast::FunctionArguments::List(list) => {
            if let Some(arg) = list.args.first() {
                match get_function_arg_expr(arg) {
                    Some(FunctionArgExpr::Expr(e)) => {
                        if let SqlExpr::Identifier(ident) = e {
                            Ok((ident.value.clone(), false))
                        } else {
                            Err(format!("Unsupported aggregate argument: {}", e).into())
                        }
                    }
                    Some(FunctionArgExpr::Wildcard) => Ok((first_col.to_string(), true)),
                    _ => Err("Unsupported function argument".into()),
                }
            } else {
                Ok((first_col.to_string(), false))
            }
        }
        _ => Ok((first_col.to_string(), false)),
    }
}

fn collect_agg_referenced_columns(
    _items: &[SelectItem],
    group_by_cols: &[String],
    aggs: &[(AggOp, String, String)],
) -> Vec<String> {
    let mut cols: Vec<String> = group_by_cols.to_vec();
    for (_, input_col, _) in aggs {
        if !cols.contains(input_col) {
            cols.push(input_col.clone());
        }
    }
    cols
}

fn extract_output_cols(items: &[SelectItem]) -> Vec<String> {
    let mut cols = Vec::new();
    for item in items {
        match item {
            SelectItem::UnnamedExpr(expr) => {
                if let SqlExpr::Identifier(ident) = expr {
                    cols.push(ident.value.clone());
                } else if let SqlExpr::Function(f) = expr {
                    let func_name = f.name.to_string().to_uppercase();
                    let display = match &f.args {
                        sqlparser::ast::FunctionArguments::List(list) => {
                            if let Some(arg) = list.args.first() {
                                match get_function_arg_expr(arg) {
                                    Some(FunctionArgExpr::Expr(SqlExpr::Identifier(ident))) => {
                                        format!("{}({})", func_name, ident.value)
                                    }
                                    Some(FunctionArgExpr::Wildcard) => format!("{}(*)", func_name),
                                    _ => func_name,
                                }
                            } else {
                                func_name
                            }
                        }
                        _ => func_name,
                    };
                    cols.push(display);
                }
            }
            SelectItem::ExprWithAlias { alias, .. } => {
                cols.push(alias.value.clone());
            }
            _ => {}
        }
    }
    cols
}

fn print_result_set(rs: &mut ResultSet) {
    let mut all_rows: Vec<Vec<String>> = Vec::new();
    let mut headers: Vec<String> = Vec::new();
    let mut first = true;

    while let Some(batch) = rs.next_batch() {
        if first {
            for col in &batch.columns {
                headers.push(col.field.name.clone());
            }
            first = false;
        }
        for row in 0..batch.num_rows {
            let mut row_data = Vec::new();
            for col in &batch.columns {
                match &col.data {
                    ColumnData::Int64(v) => row_data.push(v[row].to_string()),
                    ColumnData::Float64(v) => {
                        let val = v[row];
                        if val.fract() == 0.0 && val.abs() < 1_000_000_0.0 {
                            row_data.push(format!("{:.1}", val));
                        } else {
                            row_data.push(format!("{:.2}", val));
                        }
                    }
                    ColumnData::Utf8(v) => row_data.push(v[row].clone()),
                    ColumnData::Timestamp(v) => row_data.push(v[row].to_string()),
                }
            }
            all_rows.push(row_data);
        }
    }

    if headers.is_empty() {
        println!("(no results)");
        return;
    }

    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in &all_rows {
        for (i, val) in row.iter().enumerate() {
            widths[i] = widths[i].max(val.len());
        }
    }

    for (i, header) in headers.iter().enumerate() {
        print!("| {:^width$} ", header, width = widths[i]);
    }
    println!("|");

    for w in &widths {
        print!("|{:-^width$}", "", width = w + 2);
    }
    println!("|");

    for row in &all_rows {
        for (i, val) in row.iter().enumerate() {
            print!("| {:width$} ", val, width = widths[i]);
        }
        println!("|");
    }

    if all_rows.len() == 1 {
        println!("(1 row)");
    } else {
        println!("({} rows)", all_rows.len());
    }
}
