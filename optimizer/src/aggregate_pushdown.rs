// AggregatePushdown rule
//
// Calcite ref: AggregateJoinTransposeRule

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Pushes aggregation below a join when possible.
///
/// Example:
///   Aggregate(group_by=[a.id], SUM(b.val), InnerJoin(a, b, a.id = b.aid))
///     → InnerJoin(a, Aggregate(group_by=[b.aid], SUM(b.val), b), a.id = b.aid)
///
/// Pre-aggregating one side reduces the number of rows entering the join.
pub struct AggregatePushdown;

impl OptimizerRule for AggregatePushdown {
    fn name(&self) -> &str { "AggregatePushdown" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree. For Aggregate over Join:
        // 1. Check if all aggregate function args reference only one side of the join.
        // 2. Check if group_by keys include the join key.
        // 3. If both conditions met, push the Aggregate below the join on the relevant side.
        // 4. Adjust the outer Aggregate to combine partial results if needed.
        todo!("implement AggregatePushdown")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_push_aggregate_below_join() { todo!() }

    #[test]
    #[ignore]
    fn test_no_push_when_args_span_both_sides() { todo!() }
}
