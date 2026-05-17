// AggregateMerge rule
//
// Calcite ref: AggregateRemoveRule (related)

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Merges consecutive Aggregate nodes into a single Aggregate.
///
/// Example:
///   Aggregate(group_by=[a], SUM(x), Aggregate(group_by=[a, b], SUM(x), input))
///     → Aggregate(group_by=[a], SUM(x), input)
///   (if the outer groups are a subset of the inner groups)
pub struct AggregateMerge;

impl OptimizerRule for AggregateMerge {
    fn name(&self) -> &str { "AggregateMerge" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree. For Aggregate over Aggregate:
        // 1. Check if outer group_by is a subset of inner group_by.
        // 2. Check if aggregate functions are compatible for merging.
        // 3. If yes, combine into a single Aggregate with the outer's group_by.
        todo!("implement AggregateMerge")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_merge_compatible_aggregates() { todo!() }

    #[test]
    #[ignore]
    fn test_no_merge_incompatible() { todo!() }
}
