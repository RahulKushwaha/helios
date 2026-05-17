use std::collections::HashMap;

use expr::expr::Expr;
use expr::schema::Schema;
use expr::types::FieldValue;

use crate::aggregation::Aggregator;
use crate::engine::{ExecutionError, ResultSet, Row};
use crate::evaluator::eval;
use crate::helpers::compute_aggregate;
use crate::stream::Batch;

/// Hash-based aggregation: builds a hash table keyed by group_by columns.
/// Best for unsorted input with moderate group cardinality.
pub fn execute_hash_aggregate(
    rows: &[Row],
    schema: &Schema,
    group_by: &[Expr],
    aggr_exprs: &[Expr],
) -> Result<ResultSet, ExecutionError> {
    let mut groups: HashMap<Vec<FieldValue>, Vec<Vec<FieldValue>>> = HashMap::new();
    for row in rows {
        let key_vals: Vec<FieldValue> = group_by
            .iter()
            .map(|e| eval(e, row, schema).unwrap_or(FieldValue::Null))
            .collect();
        let entry = groups.entry(key_vals).or_insert_with(Vec::new);
        let agg_inputs: Vec<FieldValue> = aggr_exprs
            .iter()
            .map(|e| match e {
                Expr::AggregateFunction { args, .. } => args
                    .first()
                    .map(|a| eval(a, row, schema).unwrap_or(FieldValue::Null))
                    .unwrap_or(FieldValue::Null),
                _ => eval(e, row, schema).unwrap_or(FieldValue::Null),
            })
            .collect();
        entry.push(agg_inputs);
    }

    let mut result = Vec::new();
    for (group_vals, agg_rows) in groups {
        let mut out_row = group_vals;
        for (ai, agg_expr) in aggr_exprs.iter().enumerate() {
            let val = match agg_expr {
                Expr::AggregateFunction { fun, .. } => {
                    let vals: Vec<&FieldValue> = agg_rows.iter().map(|r| &r[ai]).collect();
                    compute_aggregate(fun, &vals)
                }
                _ => FieldValue::Null,
            };
            out_row.push(val);
        }
        result.push(out_row);
    }
    Ok(result)
}

/// Hash-based aggregation as an `Aggregator`. Buffers input into a hash table
/// keyed by group_by values; emits one row per group on `finalize`.
///
/// **Pipeline type:** blocking — produces no output during `accumulate`.
/// **Memory:** O(distinct group count).
pub struct HashAggregator {
    pub input_schema: Schema,
    pub group_by: Vec<Expr>,
    pub aggr_exprs: Vec<Expr>,
    // TODO: add internal accumulator state (e.g.
    //   groups: HashMap<Vec<FieldValue>, Vec<Vec<FieldValue>>>
    // — the existing free function in this file shows the layout).
}

impl HashAggregator {
    pub fn new(input_schema: Schema, group_by: Vec<Expr>, aggr_exprs: Vec<Expr>) -> Self {
        Self {
            input_schema,
            group_by,
            aggr_exprs,
        }
    }
}

impl Aggregator for HashAggregator {
    fn accumulate(&mut self, _batch: &Batch) -> Result<Option<Batch>, ExecutionError> {
        // TODO: For each row in the batch:
        //   1. Evaluate `self.group_by` to form a `Vec<FieldValue>` key.
        //   2. Evaluate the inner expressions of each aggregate.
        //   3. Insert/append into the hash table.
        // Hash aggregation is blocking: return Ok(None) here.
        // See `execute_hash_aggregate` above for the row-level logic.
        todo!("HashAggregator::accumulate")
    }

    fn finalize(&mut self) -> Result<Option<Batch>, ExecutionError> {
        // TODO: Walk the hash table; for each group, compute each aggregate
        // function over its accumulated values; emit one row per group:
        // [group_key_cols..., agg_results...].
        //
        // You can emit everything in one batch on the first call, then
        // return Ok(None) on subsequent calls. Or chunk it across calls.
        todo!("HashAggregator::finalize")
    }
}
