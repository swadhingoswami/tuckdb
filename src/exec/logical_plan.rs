use crate::exec::expr::Expr;

#[derive(Debug, Clone)]
pub enum AggOp {
    Sum,
    Count,
    Avg,
    Min,
    Max,
}

#[derive(Debug, Clone)]
pub enum LogicalPlan {
    Scan {
        table: String,
        projection: Vec<String>,
        filter: Option<Expr>,
    },
    Filter {
        input: Box<LogicalPlan>,
        predicate: Expr,
    },
    Project {
        input: Box<LogicalPlan>,
        columns: Vec<String>,
    },
    Aggregate {
        input: Box<LogicalPlan>,
        aggs: Vec<(AggOp, String, String)>, // (op, input_col, output_name)
        group_by: Vec<String>,
    },
}

impl LogicalPlan {
    pub fn scan(table: &str) -> Self {
        LogicalPlan::Scan {
            table: table.to_string(),
            projection: Vec::new(),
            filter: None,
        }
    }

    pub fn filter(self, predicate: Expr) -> Self {
        LogicalPlan::Filter {
            input: Box::new(self),
            predicate,
        }
    }

    pub fn project(self, columns: &[&str]) -> Self {
        LogicalPlan::Project {
            input: Box::new(self),
            columns: columns.iter().map(|c| c.to_string()).collect(),
        }
    }

    pub fn aggregate(self, aggs: Vec<(AggOp, &str, &str)>, group_by: Vec<String>) -> Self {
        LogicalPlan::Aggregate {
            input: Box::new(self),
            aggs: aggs
                .into_iter()
                .map(|(op, col, name)| (op, col.to_string(), name.to_string()))
                .collect(),
            group_by: group_by.to_vec(),
        }
    }
}
