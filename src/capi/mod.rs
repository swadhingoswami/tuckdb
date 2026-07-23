use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::ptr;
use std::sync::Mutex;

use sqlparser::ast::{
    BinaryOperator, Expr as SqlExpr, SelectItem, SetExpr, Statement, TableFactor, Value as SqlValue,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::api::table::Table;
use crate::exec::batch::{Column, ColumnData, RecordBatch};
use crate::exec::expr::Expr;
use crate::exec::expr::Value as TkValue;
use crate::exec::logical_plan::LogicalPlan;
use crate::schema::{DataType, Field, Schema};

static LAST_ERROR: Mutex<Option<CString>> = Mutex::new(None);

fn set_error(msg: &str) {
    if let Ok(mut err) = LAST_ERROR.lock() {
        *err = Some(CString::new(msg).unwrap_or_else(|_| CString::new("error").unwrap()));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn tuckdb_error_message(_code: i32) -> *const libc::c_char {
    if let Ok(err) = LAST_ERROR.lock() {
        if let Some(ref cstr) = *err {
            return cstr.as_ptr();
        }
    }
    ptr::null()
}

#[repr(C)]
pub struct tuckdb_table_t {
    inner: Mutex<Table>,
}

#[repr(C)]
pub struct tuckdb_result_t {
    num_rows: usize,
    num_cols: usize,
    column_names: Vec<CString>,
    columns: Vec<ColumnData>,
    string_cache: Vec<CString>,
}

fn parse_schema_json(json_str: &str) -> Result<Schema, String> {
    let s = json_str.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return Err("Schema JSON must be an array".to_string());
    }
    let inner = s[1..s.len() - 1].trim();
    if inner.is_empty() {
        return Ok(Schema::new(vec![]));
    }

    let mut fields = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    let bytes = inner.as_bytes();
    let len = bytes.len();
    for i in 0..len {
        match bytes[i] as char {
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            ',' if depth == 0 => {
                let obj_str = inner[start..i].trim();
                fields.push(parse_field_object(obj_str)?);
                start = i + 1;
            }
            _ => {}
        }
    }
    let obj_str = inner[start..].trim();
    if !obj_str.is_empty() {
        fields.push(parse_field_object(obj_str)?);
    }

    Ok(Schema::new(fields))
}

fn parse_field_object(s: &str) -> Result<Field, String> {
    let s = s.trim();
    if !s.starts_with('{') || !s.ends_with('}') {
        return Err("Field must be a JSON object".to_string());
    }
    let inner = s[1..s.len() - 1].trim();
    let mut name = String::new();
    let mut ty = String::new();
    let mut key = String::new();
    let mut val = String::new();
    let mut in_key = true;
    let mut in_str = false;

    for c in inner.chars() {
        match c {
            '"' => in_str = !in_str,
            ':' if !in_str => in_key = false,
            ',' if !in_str => {
                let k = key.trim().trim_matches('"');
                let v = val.trim().trim_matches('"');
                match k {
                    "name" => name = v.to_string(),
                    "type" => ty = v.to_string(),
                    _ => {}
                }
                key.clear();
                val.clear();
                in_key = true;
            }
            c if !in_str && c.is_whitespace() => {}
            c => {
                if in_key {
                    key.push(c);
                } else {
                    val.push(c);
                }
            }
        }
    }
    let k = key.trim().trim_matches('"');
    let v = val.trim().trim_matches('"');
    match k {
        "name" => name = v.to_string(),
        "type" => ty = v.to_string(),
        _ => {}
    }

    if name.is_empty() {
        return Err("Field missing 'name'".to_string());
    }
    let data_type = match ty.to_lowercase().as_str() {
        "int64" => DataType::Int64,
        "float64" | "double" => DataType::Float64,
        "utf8" | "string" => DataType::Utf8,
        "timestamp" => DataType::Timestamp,
        _ => return Err(format!("Unknown type '{}' for field '{}'", ty, name)),
    };

    Ok(Field::new(&name, data_type, true))
}

fn parse_csv_names(s: *const libc::c_char) -> Vec<String> {
    if s.is_null() {
        return vec![];
    }
    let cstr = unsafe { CStr::from_ptr(s) };
    let s = match cstr.to_str() {
        Ok(s) => s.trim(),
        Err(_) => return vec![],
    };
    if s.is_empty() {
        return vec![];
    }
    s.split(',')
        .map(|part| part.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn sql_to_tk_expr(e: &SqlExpr) -> Result<Expr, String> {
    match e {
        SqlExpr::Identifier(id) => Ok(Expr::Column(id.value.clone())),
        SqlExpr::Value(vws) => match &vws.value {
            SqlValue::Number(n, _) => {
                if n.contains('.') {
                    n.parse::<f64>()
                        .map(|v| Expr::Literal(TkValue::Float(v)))
                        .map_err(|e| format!("Invalid number: {}", e))
                } else {
                    n.parse::<i64>()
                        .map(|v| Expr::Literal(TkValue::Int(v)))
                        .map_err(|e| format!("Invalid number: {}", e))
                }
            }
            SqlValue::SingleQuotedString(s) => Ok(Expr::Literal(TkValue::Str(s.clone()))),
            _ => Err("Unsupported literal value".to_string()),
        },
        SqlExpr::BinaryOp { left, op, right } => {
            let l = sql_to_tk_expr(left)?;
            let r = sql_to_tk_expr(right)?;
            match op {
                BinaryOperator::Eq => Ok(Expr::Eq(Box::new(l), Box::new(r))),
                BinaryOperator::NotEq => Ok(Expr::Neq(Box::new(l), Box::new(r))),
                BinaryOperator::Gt => Ok(Expr::Gt(Box::new(l), Box::new(r))),
                BinaryOperator::GtEq => Ok(Expr::Gte(Box::new(l), Box::new(r))),
                BinaryOperator::Lt => Ok(Expr::Lt(Box::new(l), Box::new(r))),
                BinaryOperator::LtEq => Ok(Expr::Lte(Box::new(l), Box::new(r))),
                BinaryOperator::And => Ok(Expr::And(Box::new(l), Box::new(r))),
                BinaryOperator::Or => Ok(Expr::Or(Box::new(l), Box::new(r))),
                _ => Err("Unsupported binary operator".to_string()),
            }
        }
        SqlExpr::Nested(inner) => sql_to_tk_expr(inner),
        _ => Err("Unsupported SQL expression".to_string()),
    }
}

fn sql_to_logical_plan(sql: &str, default_table: &str) -> Result<LogicalPlan, String> {
    let dialect = GenericDialect;
    let mut stmts =
        Parser::parse_sql(&dialect, sql).map_err(|e| format!("SQL parse error: {}", e))?;

    if stmts.len() != 1 {
        return Err("Expected a single SQL statement".to_string());
    }
    let stmt = stmts.remove(0);

    let select = match &stmt {
        Statement::Query(q) => match &*q.body {
            SetExpr::Select(s) => s,
            _ => return Err("Only SELECT queries are supported".to_string()),
        },
        _ => return Err("Only SELECT queries are supported".to_string()),
    };

    if select.from.is_empty() {
        return Err("FROM clause is required".to_string());
    }

    // Validate FROM clause references the correct table
    let from = &select.from[0];
    let sql_table = match &from.relation {
        TableFactor::Table { name, .. } => name.0[0].as_ident().unwrap().value.clone(),
        _ => return Err("Only simple table references are supported".to_string()),
    };
    if sql_table != default_table {
        return Err(format!(
            "Table '{}' not found (expected '{}')",
            sql_table, default_table
        ));
    }

    let projection = if select.projection.is_empty() {
        vec![]
    } else if select
        .projection
        .iter()
        .any(|item| matches!(item, SelectItem::Wildcard(_)))
    {
        vec![]
    } else {
        let mut cols = Vec::new();
        for item in &select.projection {
            match item {
                SelectItem::UnnamedExpr(SqlExpr::Identifier(id)) => {
                    cols.push(id.value.clone());
                }
                SelectItem::ExprWithAlias {
                    expr: SqlExpr::Identifier(_id),
                    alias,
                } => {
                    cols.push(alias.value.clone());
                }
                _ => return Err("Only column references are supported in SELECT".to_string()),
            }
        }
        cols
    };

    let filter = match &select.selection {
        Some(expr) => Some(sql_to_tk_expr(expr)?),
        None => None,
    };

    Ok(LogicalPlan::Scan {
        table: default_table.to_string(),
        projection,
        filter,
    })
}

fn col_as_f64(col: &ColumnData, row: usize) -> Option<f64> {
    match col {
        ColumnData::Int64(v) => v.get(row).map(|&x| x as f64),
        ColumnData::Float64(v) => v.get(row).copied(),
        ColumnData::Utf8(v) => v.get(row).and_then(|s| s.parse::<f64>().ok()),
        ColumnData::Timestamp(v) => v.get(row).map(|&x| x as f64),
    }
}

fn col_as_i64(col: &ColumnData, row: usize) -> Option<i64> {
    match col {
        ColumnData::Int64(v) => v.get(row).copied(),
        ColumnData::Float64(v) => v.get(row).map(|&x| x as i64),
        ColumnData::Utf8(v) => v.get(row).and_then(|s| s.parse::<i64>().ok()),
        ColumnData::Timestamp(v) => v.get(row).copied(),
    }
}

fn build_string_cache(columns: &[ColumnData], num_rows: usize) -> Vec<CString> {
    let mut cache = Vec::with_capacity(columns.len() * num_rows);
    for col in columns {
        for row in 0..num_rows {
            let s = match col {
                ColumnData::Int64(v) => v[row].to_string(),
                ColumnData::Float64(v) => v[row].to_string(),
                ColumnData::Utf8(v) => v[row].clone(),
                ColumnData::Timestamp(v) => v[row].to_string(),
            };
            cache.push(CString::new(s).unwrap_or_else(|_| CString::new("").unwrap()));
        }
    }
    cache
}

#[unsafe(no_mangle)]
pub extern "C" fn tuckdb_create(
    name: *const libc::c_char,
    schema_json: *const libc::c_char,
    path: *const libc::c_char,
) -> *mut tuckdb_table_t {
    if name.is_null() || schema_json.is_null() {
        set_error("name and schema_json must not be null");
        return ptr::null_mut();
    }

    let name = match unsafe { CStr::from_ptr(name) }.to_str() {
        Ok(s) => s.to_string(),
        Err(e) => {
            set_error(&format!("Invalid name: {}", e));
            return ptr::null_mut();
        }
    };

    let schema_str = match unsafe { CStr::from_ptr(schema_json) }.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_error(&format!("Invalid schema_json: {}", e));
            return ptr::null_mut();
        }
    };

    let schema = match parse_schema_json(schema_str) {
        Ok(s) => s,
        Err(e) => {
            set_error(&e);
            return ptr::null_mut();
        }
    };

    let base_path = if path.is_null() {
        PathBuf::from(".")
    } else {
        match unsafe { CStr::from_ptr(path) }.to_str() {
            Ok(s) => PathBuf::from(s),
            Err(e) => {
                set_error(&format!("Invalid path: {}", e));
                return ptr::null_mut();
            }
        }
    };

    let table = Table::create(&name, schema, base_path);
    Box::into_raw(Box::new(tuckdb_table_t {
        inner: Mutex::new(table),
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn tuckdb_open(
    name: *const libc::c_char,
    path: *const libc::c_char,
) -> *mut tuckdb_table_t {
    if name.is_null() {
        set_error("name must not be null");
        return ptr::null_mut();
    }

    let name = match unsafe { CStr::from_ptr(name) }.to_str() {
        Ok(s) => s.to_string(),
        Err(e) => {
            set_error(&format!("Invalid name: {}", e));
            return ptr::null_mut();
        }
    };

    let base_path = if path.is_null() {
        PathBuf::from(".")
    } else {
        match unsafe { CStr::from_ptr(path) }.to_str() {
            Ok(s) => PathBuf::from(s),
            Err(e) => {
                set_error(&format!("Invalid path: {}", e));
                return ptr::null_mut();
            }
        }
    };

    let table = Table::open(&name, base_path);
    Box::into_raw(Box::new(tuckdb_table_t {
        inner: Mutex::new(table),
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn tuckdb_insert(
    table: *mut tuckdb_table_t,
    num_columns: libc::c_int,
    int_cols: *const i64,
    int_col_names: *const libc::c_char,
    float_cols: *const f64,
    float_col_names: *const libc::c_char,
    str_cols: *const *const libc::c_char,
    str_col_names: *const libc::c_char,
) -> libc::c_int {
    if table.is_null() {
        set_error("table is null");
        return -1;
    }

    let tbl = unsafe { &*table };
    let int_names = parse_csv_names(int_col_names);
    let float_names = parse_csv_names(float_col_names);
    let str_names = parse_csv_names(str_col_names);

    let total_names = int_names.len() + float_names.len() + str_names.len();
    if total_names != num_columns as usize {
        set_error(&format!(
            "column count mismatch: got {} names, expected {}",
            total_names, num_columns
        ));
        return -1;
    }

    let int_values: Vec<i64> = if int_cols.is_null() {
        vec![]
    } else {
        (0..int_names.len())
            .map(|i| unsafe { *int_cols.add(i) })
            .collect()
    };

    let float_values: Vec<f64> = if float_cols.is_null() {
        vec![]
    } else {
        (0..float_names.len())
            .map(|i| unsafe { *float_cols.add(i) })
            .collect()
    };

    let str_values: Vec<String> = if str_cols.is_null() {
        vec![]
    } else {
        (0..str_names.len())
            .map(|i| {
                let p = unsafe { *str_cols.add(i) };
                if p.is_null() {
                    String::new()
                } else {
                    unsafe { CStr::from_ptr(p) }
                        .to_str()
                        .unwrap_or("")
                        .to_string()
                }
            })
            .collect()
    };

    let mut guard = match tbl.inner.lock() {
        Ok(g) => g,
        Err(e) => {
            set_error(&format!("lock error: {}", e));
            return -1;
        }
    };

    let schema = guard.schema().clone();
    if schema.fields.len() != total_names {
        set_error(&format!(
            "expected {} columns in schema, got {}",
            schema.fields.len(),
            total_names
        ));
        return -1;
    }

    let mut columns = Vec::with_capacity(schema.fields.len());
    for field in &schema.fields {
        let col = match field.data_type {
            DataType::Int64 => match int_names.iter().position(|n| n == &field.name) {
                Some(idx) => Column::new(field.clone(), ColumnData::Int64(vec![int_values[idx]])),
                None => {
                    set_error(&format!("Missing int64 column '{}'", field.name));
                    return -1;
                }
            },
            DataType::Float64 => match float_names.iter().position(|n| n == &field.name) {
                Some(idx) => {
                    Column::new(field.clone(), ColumnData::Float64(vec![float_values[idx]]))
                }
                None => {
                    set_error(&format!("Missing float64 column '{}'", field.name));
                    return -1;
                }
            },
            DataType::Utf8 => match str_names.iter().position(|n| n == &field.name) {
                Some(idx) => Column::new(
                    field.clone(),
                    ColumnData::Utf8(vec![str_values[idx].clone()]),
                ),
                None => {
                    set_error(&format!("Missing utf8 column '{}'", field.name));
                    return -1;
                }
            },
            DataType::Timestamp => match int_names.iter().position(|n| n == &field.name) {
                Some(idx) => {
                    Column::new(field.clone(), ColumnData::Timestamp(vec![int_values[idx]]))
                }
                None => {
                    set_error(&format!("Missing timestamp column '{}'", field.name));
                    return -1;
                }
            },
        };
        columns.push(col);
    }

    let batch = RecordBatch::new(schema, columns);
    guard.insert_batch(batch);
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn tuckdb_query(
    table: *mut tuckdb_table_t,
    sql: *const libc::c_char,
) -> *mut tuckdb_result_t {
    if table.is_null() || sql.is_null() {
        set_error("table and sql must not be null");
        return ptr::null_mut();
    }

    let sql_str = match unsafe { CStr::from_ptr(sql) }.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_error(&format!("Invalid sql: {}", e));
            return ptr::null_mut();
        }
    };

    let tbl = unsafe { &*table };
    let mut guard = match tbl.inner.lock() {
        Ok(g) => g,
        Err(e) => {
            set_error(&format!("lock error: {}", e));
            return ptr::null_mut();
        }
    };

    let table_name = guard.name().to_string();
    let plan = match sql_to_logical_plan(sql_str, &table_name) {
        Ok(p) => p,
        Err(e) => {
            set_error(&e);
            return ptr::null_mut();
        }
    };

    let result_set = guard.execute(plan);
    let batches = result_set.collect();

    if batches.is_empty() {
        let schema = guard.schema().clone();
        let column_names: Vec<CString> = schema
            .fields
            .iter()
            .map(|f| CString::new(f.name.clone()).unwrap_or_default())
            .collect();
        let num_cols = column_names.len();
        return Box::into_raw(Box::new(tuckdb_result_t {
            num_rows: 0,
            num_cols,
            column_names,
            columns: vec![],
            string_cache: vec![],
        }));
    }

    let schema = batches[0].schema.clone();
    let num_cols = schema.fields.len();
    let column_names: Vec<CString> = schema
        .fields
        .iter()
        .map(|f| CString::new(f.name.clone()).unwrap_or_default())
        .collect();

    let mut flat: Vec<Vec<ColumnData>> = (0..num_cols).map(|_| vec![]).collect();
    for batch in &batches {
        for (ci, col) in batch.columns.iter().enumerate() {
            flat[ci].push(col.data.clone());
        }
    }

    let mut total_rows = 0usize;
    let mut columns = Vec::with_capacity(num_cols);
    for col_vec in flat {
        if col_vec.is_empty() {
            columns.push(ColumnData::Int64(vec![]));
            continue;
        }
        match &col_vec[0] {
            ColumnData::Int64(_) => {
                let mut all = Vec::new();
                for cd in &col_vec {
                    if let ColumnData::Int64(v) = cd {
                        all.extend_from_slice(v);
                    }
                }
                total_rows = all.len();
                columns.push(ColumnData::Int64(all));
            }
            ColumnData::Float64(_) => {
                let mut all = Vec::new();
                for cd in &col_vec {
                    if let ColumnData::Float64(v) = cd {
                        all.extend_from_slice(v);
                    }
                }
                total_rows = all.len();
                columns.push(ColumnData::Float64(all));
            }
            ColumnData::Utf8(_) => {
                let mut all = Vec::new();
                for cd in &col_vec {
                    if let ColumnData::Utf8(v) = cd {
                        all.extend(v.iter().cloned());
                    }
                }
                total_rows = all.len();
                columns.push(ColumnData::Utf8(all));
            }
            ColumnData::Timestamp(_) => {
                let mut all = Vec::new();
                for cd in &col_vec {
                    if let ColumnData::Timestamp(v) = cd {
                        all.extend_from_slice(v);
                    }
                }
                total_rows = all.len();
                columns.push(ColumnData::Timestamp(all));
            }
        }
    }

    let string_cache = build_string_cache(&columns, total_rows);
    Box::into_raw(Box::new(tuckdb_result_t {
        num_rows: total_rows,
        num_cols,
        column_names,
        columns,
        string_cache,
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn tuckdb_result_num_rows(result: *const tuckdb_result_t) -> libc::c_int {
    if result.is_null() {
        return 0;
    }
    unsafe { (*result).num_rows as libc::c_int }
}

#[unsafe(no_mangle)]
pub extern "C" fn tuckdb_result_num_cols(result: *const tuckdb_result_t) -> libc::c_int {
    if result.is_null() {
        return 0;
    }
    unsafe { (*result).num_cols as libc::c_int }
}

#[unsafe(no_mangle)]
pub extern "C" fn tuckdb_result_column_name(
    result: *const tuckdb_result_t,
    col: libc::c_int,
) -> *const libc::c_char {
    if result.is_null() {
        return ptr::null();
    }
    let r = unsafe { &*result };
    if col < 0 || col as usize >= r.num_cols {
        return ptr::null();
    }
    r.column_names[col as usize].as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn tuckdb_result_value_double(
    result: *const tuckdb_result_t,
    row: libc::c_int,
    col: libc::c_int,
) -> f64 {
    if result.is_null() {
        return 0.0;
    }
    let r = unsafe { &*result };
    if row < 0 || row as usize >= r.num_rows || col < 0 || col as usize >= r.num_cols {
        return 0.0;
    }
    col_as_f64(&r.columns[col as usize], row as usize).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn tuckdb_result_value_int(
    result: *const tuckdb_result_t,
    row: libc::c_int,
    col: libc::c_int,
) -> i64 {
    if result.is_null() {
        return 0;
    }
    let r = unsafe { &*result };
    if row < 0 || row as usize >= r.num_rows || col < 0 || col as usize >= r.num_cols {
        return 0;
    }
    col_as_i64(&r.columns[col as usize], row as usize).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn tuckdb_result_value_string(
    result: *const tuckdb_result_t,
    row: libc::c_int,
    col: libc::c_int,
) -> *const libc::c_char {
    if result.is_null() {
        return ptr::null();
    }
    let r = unsafe { &*result };
    if row < 0 || row as usize >= r.num_rows || col < 0 || col as usize >= r.num_cols {
        return ptr::null();
    }
    r.string_cache[col as usize * r.num_rows + row as usize].as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn tuckdb_table_free(table: *mut tuckdb_table_t) {
    if !table.is_null() {
        unsafe {
            drop(Box::from_raw(table));
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn tuckdb_result_free(result: *mut tuckdb_result_t) {
    if !result.is_null() {
        unsafe {
            drop(Box::from_raw(result));
        }
    }
}
