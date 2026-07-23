use crate::exec::expr::Expr;
use crate::exec::logical_plan::LogicalPlan;
use crate::schema::Schema;

pub struct Optimizer;

impl Optimizer {
    pub fn optimize(plan: LogicalPlan, schema: &Schema) -> LogicalPlan {
        let plan = Self::push_projection_into_scan(plan, schema);
        let plan = Self::push_filter_into_scan(plan);
        plan
    }

    /// Push projections into Scan nodes so only needed columns are read.
    fn push_projection_into_scan(plan: LogicalPlan, schema: &Schema) -> LogicalPlan {
        match plan {
            LogicalPlan::Project { input, columns } => {
                let new_input = Self::push_projection_into_scan(*input, schema);
                match new_input {
                    LogicalPlan::Scan {
                        table,
                        projection: _,
                        filter,
                    } => {
                        // merge projections: only keep columns that are in the project
                        let new_proj: Vec<String> = columns
                            .iter()
                            .filter(|c| schema.index_of(c).is_some())
                            .cloned()
                            .collect();
                        LogicalPlan::Project {
                            input: Box::new(LogicalPlan::Scan {
                                table,
                                projection: new_proj,
                                filter,
                            }),
                            columns,
                        }
                    }
                    other => LogicalPlan::Project {
                        input: Box::new(other),
                        columns,
                    },
                }
            }
            LogicalPlan::Filter { input, predicate } => {
                let new_input = Self::push_projection_into_scan(*input, schema);
                LogicalPlan::Filter {
                    input: Box::new(new_input),
                    predicate,
                }
            }
            LogicalPlan::Aggregate {
                input,
                aggs,
                group_by,
            } => {
                let new_input = Self::push_projection_into_scan(*input, schema);
                LogicalPlan::Aggregate {
                    input: Box::new(new_input),
                    aggs,
                    group_by,
                }
            }
            other => other,
        }
    }

    /// Push Filter predicates into Scan nodes to enable chunk skipping.
    fn push_filter_into_scan(plan: LogicalPlan) -> LogicalPlan {
        match plan {
            LogicalPlan::Filter { input, predicate } => {
                let new_input = Self::push_filter_into_scan(*input);
                match new_input {
                    LogicalPlan::Scan {
                        table,
                        projection,
                        filter: existing,
                    } => {
                        let combined = match existing {
                            Some(e) => Expr::And(Box::new(e), Box::new(predicate)),
                            None => predicate,
                        };
                        LogicalPlan::Scan {
                            table,
                            projection,
                            filter: Some(combined),
                        }
                    }
                    other => LogicalPlan::Filter {
                        input: Box::new(other),
                        predicate,
                    },
                }
            }
            LogicalPlan::Project { input, columns } => {
                let new_input = Self::push_filter_into_scan(*input);
                LogicalPlan::Project {
                    input: Box::new(new_input),
                    columns,
                }
            }
            LogicalPlan::Aggregate {
                input,
                aggs,
                group_by,
            } => {
                let new_input = Self::push_filter_into_scan(*input);
                LogicalPlan::Aggregate {
                    input: Box::new(new_input),
                    aggs,
                    group_by,
                }
            }
            other => other,
        }
    }
}
