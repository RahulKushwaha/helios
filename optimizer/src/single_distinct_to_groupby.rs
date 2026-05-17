// SingleDistinctToGroupBy rule
//
// DataFusion ref: datafusion/optimizer/src/single_distinct_to_groupby.rs

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Rewrites COUNT(DISTINCT x) into a two-phase GROUP BY.
///
/// Example:
///   Aggregate(group_by=[], [COUNT(DISTINCT x)], input)
///     → Aggregate(group_by=[], [COUNT(x)],
///         Aggregate(group_by=[x], [], input))
///
/// Phase 1 (inner): GROUP BY x to deduplicate.
/// Phase 2 (outer): COUNT the distinct groups.
pub struct SingleDistinctToGroupBy;

impl OptimizerRule for SingleDistinctToGroupBy {
    fn name(&self) -> &str { "SingleDistinctToGroupBy" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree. For each Aggregate:
        // 1. Check if any aggr_expr is a "distinct" aggregate.
        //    (You may need to add a `distinct: bool` field to AggregateFunction.)
        // 2. If exactly one distinct aggregate exists:
        //    a. Create inner Aggregate: GROUP BY the distinct column, no aggr_exprs.
        //    b. Create outer Aggregate: apply the non-distinct version of the function.
        // 3. If multiple distinct aggregates on different columns → skip (not supported).
        todo!("implement SingleDistinctToGroupBy")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_rewrite_count_distinct() { todo!() }

    #[test]
    #[ignore]
    fn test_no_rewrite_without_distinct() { todo!() }
}
