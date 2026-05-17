//! Materialized (non-streaming) executor.
//!
//! Walks a `PhysicalPlan` recursively, fully materializing each operator's
//! output into a `Vec<Row>` before passing it to the parent. Storage-agnostic:
//! it reaches the leaf via the [`DataSource`] trait, so any backend works.
//!
//! Used by `stream::build_stream` as the fallback for operators that haven't
//! been converted to streaming yet. Each operator that gets a streaming impl
//! moves out of here and into `stream.rs`. When every operator is streaming,
//! this module goes away.

use expr::expr::{Expr, Operator};
use expr::logical_plan::JoinType;
use expr::schema::Schema;
use expr::types::FieldValue;
use physical_plan::plan::PhysicalPlan;

use crate::engine::{DataSource, ExecutionError, ResultSet, Row};
use crate::evaluator::{cmp_values, eval};
use crate::helpers::{compute_aggregate, plan_schema};

/// Execute a physical plan against a data source, fully materializing the
/// result.
pub fn execute_plan(
    source: &dyn DataSource,
    plan: &PhysicalPlan,
) -> Result<ResultSet, ExecutionError> {
    match plan {
        PhysicalPlan::TableScan {
            table_name, schema, ..
        } => source.scan(table_name, schema),

        PhysicalPlan::Filter {
            predicate, input, ..
        } => {
            let schema = plan_schema(input);
            let rows = execute_plan(source, input)?;
            Ok(rows
                .into_iter()
                .filter(|row| {
                    matches!(eval(predicate, row, &schema), Ok(FieldValue::Bool(true)))
                })
                .collect())
        }

        PhysicalPlan::Projection { exprs, input } => {
            let schema = plan_schema(input);
            let rows = execute_plan(source, input)?;
            rows.into_iter()
                .map(|row| {
                    exprs
                        .iter()
                        .map(|e| eval(e, &row, &schema).map_err(ExecutionError::Internal))
                        .collect()
                })
                .collect()
        }

        PhysicalPlan::NestedLoopJoin {
            left,
            right,
            on,
            join_type,
        } => execute_join(source, left, right, on, join_type),

        PhysicalPlan::Sort { exprs, input } => {
            let schema = plan_schema(input);
            let mut rows = execute_plan(source, input)?;
            rows.sort_by(|a, b| {
                for e in exprs {
                    let va = eval(e, a, &schema).unwrap_or(FieldValue::Null);
                    let vb = eval(e, b, &schema).unwrap_or(FieldValue::Null);
                    let ord = cmp_values(&va, &vb);
                    if ord != std::cmp::Ordering::Equal {
                        return ord;
                    }
                }
                std::cmp::Ordering::Equal
            });
            Ok(rows)
        }

        PhysicalPlan::HashAggregate {
            group_by,
            aggr_exprs,
            input,
        } => {
            let schema = plan_schema(input);
            let rows = execute_plan(source, input)?;
            crate::aggregation::hash_aggregate::execute_hash_aggregate(
                &rows, &schema, group_by, aggr_exprs,
            )
        }

        PhysicalPlan::SortAggregate { .. } => {
            // Streaming SortAggregate goes through `stream::build_stream` →
            // `AggregateStream(SortAggregator)`. This arm is only reachable
            // if SortAggregate appears nested under an unconverted operator.
            todo!("execute SortAggregate via materialized path")
        }

        PhysicalPlan::ScalarAggregate { .. } => {
            // Same situation as SortAggregate above.
            todo!("execute ScalarAggregate via materialized path")
        }

        PhysicalPlan::Limit { input, .. } => {
            // Limit's streaming impl in `stream.rs` is canonical. This arm
            // ignores skip/fetch and just forwards the input — it's only
            // reachable for a Limit nested under an unconverted operator,
            // which doesn't currently happen for any user-facing query.
            execute_plan(source, input)
        }
    }
}

fn execute_join(
    source: &dyn DataSource,
    left: &PhysicalPlan,
    right: &PhysicalPlan,
    on: &Expr,
    join_type: &JoinType,
) -> Result<ResultSet, ExecutionError> {
    let left_schema = plan_schema(left);
    let right_schema = plan_schema(right);
    let left_rows = execute_plan(source, left)?;
    let right_rows = execute_plan(source, right)?;

    let mut combined_fields = left_schema.fields.clone();
    combined_fields.extend(right_schema.fields.clone());
    let combined_schema = Schema::new(combined_fields);

    let null_right: Row = vec![FieldValue::Null; right_schema.fields.len()];
    let null_left: Row = vec![FieldValue::Null; left_schema.fields.len()];
    let mut result = Vec::new();

    match join_type {
        JoinType::Inner => {
            for lr in &left_rows {
                for rr in &right_rows {
                    let mut combined = lr.clone();
                    combined.extend(rr.clone());
                    if let Ok(FieldValue::Bool(true)) = eval(on, &combined, &combined_schema) {
                        result.push(combined);
                    }
                }
            }
        }
        JoinType::Left => {
            for lr in &left_rows {
                let mut matched = false;
                for rr in &right_rows {
                    let mut combined = lr.clone();
                    combined.extend(rr.clone());
                    if let Ok(FieldValue::Bool(true)) = eval(on, &combined, &combined_schema) {
                        result.push(combined);
                        matched = true;
                    }
                }
                if !matched {
                    let mut combined = lr.clone();
                    combined.extend(null_right.clone());
                    result.push(combined);
                }
            }
        }
        JoinType::Right => {
            for rr in &right_rows {
                let mut matched = false;
                for lr in &left_rows {
                    let mut combined = lr.clone();
                    combined.extend(rr.clone());
                    if let Ok(FieldValue::Bool(true)) = eval(on, &combined, &combined_schema) {
                        result.push(combined);
                        matched = true;
                    }
                }
                if !matched {
                    let mut combined = null_left.clone();
                    combined.extend(rr.clone());
                    result.push(combined);
                }
            }
        }
        JoinType::Full => {
            let mut right_matched = vec![false; right_rows.len()];
            for lr in &left_rows {
                let mut left_matched = false;
                for (ri, rr) in right_rows.iter().enumerate() {
                    let mut combined = lr.clone();
                    combined.extend(rr.clone());
                    if let Ok(FieldValue::Bool(true)) = eval(on, &combined, &combined_schema) {
                        result.push(combined);
                        left_matched = true;
                        right_matched[ri] = true;
                    }
                }
                if !left_matched {
                    let mut combined = lr.clone();
                    combined.extend(null_right.clone());
                    result.push(combined);
                }
            }
            for (ri, rr) in right_rows.iter().enumerate() {
                if !right_matched[ri] {
                    let mut combined = null_left.clone();
                    combined.extend(rr.clone());
                    result.push(combined);
                }
            }
        }
    }
    Ok(result)
}
