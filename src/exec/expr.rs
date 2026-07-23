use crate::exec::batch::{ColumnData, RecordBatch};

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Column(String),
    Literal(Value),
    Eq(Box<Expr>, Box<Expr>),
    Neq(Box<Expr>, Box<Expr>),
    Gt(Box<Expr>, Box<Expr>),
    Gte(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    Lte(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

impl Expr {
    pub fn eq(self, other: Expr) -> Expr {
        Expr::Eq(Box::new(self), Box::new(other))
    }

    pub fn neq(self, other: Expr) -> Expr {
        Expr::Neq(Box::new(self), Box::new(other))
    }

    pub fn gt(self, other: Expr) -> Expr {
        Expr::Gt(Box::new(self), Box::new(other))
    }

    pub fn gte(self, other: Expr) -> Expr {
        Expr::Gte(Box::new(self), Box::new(other))
    }

    pub fn lt(self, other: Expr) -> Expr {
        Expr::Lt(Box::new(self), Box::new(other))
    }

    pub fn lte(self, other: Expr) -> Expr {
        Expr::Lte(Box::new(self), Box::new(other))
    }

    pub fn and(self, other: Expr) -> Expr {
        Expr::And(Box::new(self), Box::new(other))
    }

    pub fn or(self, other: Expr) -> Expr {
        Expr::Or(Box::new(self), Box::new(other))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
}

pub fn col(name: &str) -> Expr {
    Expr::Column(name.to_string())
}

pub fn lit_int(v: i64) -> Expr {
    Expr::Literal(Value::Int(v))
}

pub fn lit_float(v: f64) -> Expr {
    Expr::Literal(Value::Float(v))
}

pub fn lit_str(v: &str) -> Expr {
    Expr::Literal(Value::Str(v.to_string()))
}

pub fn eval_expr(expr: &Expr, batch: &RecordBatch, row: usize) -> Option<Value> {
    match expr {
        Expr::Column(name) => {
            let idx = batch.schema.index_of(name)?;
            let col = &batch.columns[idx];
            match &col.data {
                ColumnData::Int64(v) => Some(Value::Int(v[row])),
                ColumnData::Float64(v) => Some(Value::Float(v[row])),
                ColumnData::Utf8(v) => Some(Value::Str(v[row].clone())),
                ColumnData::Timestamp(v) => Some(Value::Int(v[row])),
            }
        }
        Expr::Literal(v) => Some(v.clone()),
        Expr::Eq(a, b) => {
            let va = eval_expr(a, batch, row)?;
            let vb = eval_expr(b, batch, row)?;
            Some(Value::Int(if va == vb { 1 } else { 0 }))
        }
        Expr::Neq(a, b) => {
            let va = eval_expr(a, batch, row)?;
            let vb = eval_expr(b, batch, row)?;
            Some(Value::Int(if va != vb { 1 } else { 0 }))
        }
        Expr::Gt(a, b) => {
            let va = eval_expr(a, batch, row)?;
            let vb = eval_expr(b, batch, row)?;
            Some(Value::Int(if cmp_value(&va, &vb) > 0 { 1 } else { 0 }))
        }
        Expr::Gte(a, b) => {
            let va = eval_expr(a, batch, row)?;
            let vb = eval_expr(b, batch, row)?;
            Some(Value::Int(if cmp_value(&va, &vb) >= 0 { 1 } else { 0 }))
        }
        Expr::Lt(a, b) => {
            let va = eval_expr(a, batch, row)?;
            let vb = eval_expr(b, batch, row)?;
            Some(Value::Int(if cmp_value(&va, &vb) < 0 { 1 } else { 0 }))
        }
        Expr::Lte(a, b) => {
            let va = eval_expr(a, batch, row)?;
            let vb = eval_expr(b, batch, row)?;
            Some(Value::Int(if cmp_value(&va, &vb) <= 0 { 1 } else { 0 }))
        }
        Expr::And(a, b) => {
            let va = eval_expr(a, batch, row)?;
            let vb = eval_expr(b, batch, row)?;
            let ia = if let Value::Int(i) = va { i } else { 0 };
            let ib = if let Value::Int(i) = vb { i } else { 0 };
            Some(Value::Int(if ia != 0 && ib != 0 { 1 } else { 0 }))
        }
        Expr::Or(a, b) => {
            let va = eval_expr(a, batch, row)?;
            let vb = eval_expr(b, batch, row)?;
            let ia = if let Value::Int(i) = va { i } else { 0 };
            let ib = if let Value::Int(i) = vb { i } else { 0 };
            Some(Value::Int(if ia != 0 || ib != 0 { 1 } else { 0 }))
        }
    }
}

fn cmp_value(a: &Value, b: &Value) -> i32 {
    match (a, b) {
        (Value::Int(ai), Value::Int(bi)) => ai.cmp(bi) as i32,
        (Value::Float(af), Value::Float(bf)) => {
            if af < bf {
                -1
            } else if af > bf {
                1
            } else {
                0
            }
        }
        (Value::Str(as_), Value::Str(bs)) => as_.cmp(bs) as i32,
        (Value::Int(ai), Value::Float(bf)) => {
            let af = *ai as f64;
            if af < *bf {
                -1
            } else if af > *bf {
                1
            } else {
                0
            }
        }
        (Value::Float(af), Value::Int(bi)) => {
            let bf = *bi as f64;
            if *af < bf {
                -1
            } else if *af > bf {
                1
            } else {
                0
            }
        }
        _ => 0,
    }
}

pub fn eval_filter(expr: &Expr, batch: &RecordBatch) -> Vec<bool> {
    let mut mask = Vec::with_capacity(batch.num_rows);
    for row in 0..batch.num_rows {
        match eval_expr(expr, batch, row) {
            Some(Value::Int(i)) => mask.push(i != 0),
            _ => mask.push(false),
        }
    }
    mask
}

pub fn referenced_columns(expr: &Expr) -> Vec<String> {
    let mut cols = Vec::new();
    collect_columns(expr, &mut cols);
    cols
}

pub(crate) fn could_match(expr: &Expr, col_name: &str, min: f64, max: f64) -> bool {
    match expr {
        Expr::Column(name) => name == col_name,
        Expr::Literal(_) => true,
        Expr::Eq(a, b) => cmp_lit(a, b, col_name, |val| min <= val && val <= max),
        Expr::Neq(a, b) => cmp_lit(a, b, col_name, |val| !(min == max && min == val)),
        Expr::Gt(a, b) => cmp_lit(a, b, col_name, |val| max > val),
        Expr::Gte(a, b) => cmp_lit(a, b, col_name, |val| max >= val),
        Expr::Lt(a, b) => cmp_lit(a, b, col_name, |val| min < val),
        Expr::Lte(a, b) => cmp_lit(a, b, col_name, |val| min <= val),
        Expr::And(a, b) => could_match(a, col_name, min, max) && could_match(b, col_name, min, max),
        Expr::Or(a, b) => could_match(a, col_name, min, max) || could_match(b, col_name, min, max),
    }
}

fn cmp_lit(a: &Expr, b: &Expr, col_name: &str, cmp: impl Fn(f64) -> bool) -> bool {
    match (a, b) {
        (Expr::Column(name), Expr::Literal(val)) if name == col_name => match val {
            Value::Int(v) => cmp(*v as f64),
            Value::Float(v) => cmp(*v),
            _ => true,
        },
        (Expr::Literal(val), Expr::Column(name)) if name == col_name => match val {
            Value::Int(v) => cmp(*v as f64),
            Value::Float(v) => cmp(*v),
            _ => true,
        },
        _ => true,
    }
}

fn collect_columns(expr: &Expr, cols: &mut Vec<String>) {
    match expr {
        Expr::Column(name) => {
            if !cols.contains(name) {
                cols.push(name.clone());
            }
        }
        Expr::Literal(_) => {}
        Expr::Eq(a, b)
        | Expr::Neq(a, b)
        | Expr::Gt(a, b)
        | Expr::Gte(a, b)
        | Expr::Lt(a, b)
        | Expr::Lte(a, b)
        | Expr::And(a, b)
        | Expr::Or(a, b) => {
            collect_columns(a, cols);
            collect_columns(b, cols);
        }
    }
}
