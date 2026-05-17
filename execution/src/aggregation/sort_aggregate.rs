use expr::expr::Expr;
use expr::schema::Schema;

use crate::aggregation::Aggregator;
use crate::engine::{ExecutionError, ResultSet, Row};
use crate::stream::Batch;

/// Sort-based aggregation: assumes input is already sorted by group_by columns.
/// Scans sequentially, emitting a result each time the group key changes.
/// O(n) time, O(1) memory beyond the input.
pub fn execute_sort_aggregate(
    rows: &[Row],
    schema: &Schema,
    group_by: &[Expr],
    aggr_exprs: &[Expr],
) -> Result<ResultSet, ExecutionError> {
    todo!("implement sort aggregate")
}

/// Sort-based aggregation as an `Aggregator`. Assumes input arrives sorted on
/// the group_by columns, so each batch may finish one or more groups inline.
///
/// **Pipeline type:** **streaming** — the only aggregation strategy that can
/// emit completed groups during `accumulate` rather than waiting for finalize.
/// **Memory:** O(1) beyond the in-progress group's accumulators.
pub struct SortAggregator {
    pub input_schema: Schema,
    pub group_by: Vec<Expr>,
    pub aggr_exprs: Vec<Expr>,
    // TODO: add running-group state, e.g.
    //   current_key: Option<Vec<FieldValue>>,
    //   current_accumulators: Vec<...>,
}

impl SortAggregator {
    pub fn new(input_schema: Schema, group_by: Vec<Expr>, aggr_exprs: Vec<Expr>) -> Self {
        Self {
            input_schema,
            group_by,
            aggr_exprs,
        }
    }
}

impl Aggregator for SortAggregator {
    fn accumulate(&mut self, _batch: &Batch) -> Result<Option<Batch>, ExecutionError> {
        // TODO: Walk the batch row-by-row.
        // For each row:
        //   1. Compute its group key.
        //   2. If it matches the current in-progress key → fold into accumulators.
        //   3. If it differs → emit the current group as a row, reset
        //      accumulators, start the new group.
        // Return any emitted rows as a batch (or Ok(None) if none completed).
        todo!("SortAggregator::accumulate")
    }

    fn finalize(&mut self) -> Result<Option<Batch>, ExecutionError> {
        // TODO: Emit the final in-progress group (if any) and return Ok(None)
        // on subsequent calls.
        todo!("SortAggregator::finalize")
    }
}
