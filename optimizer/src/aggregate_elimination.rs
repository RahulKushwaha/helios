// AggregateElimination rule
//
// Calcite ref: AggregateRemoveRule

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Eliminates redundant aggregations.
///
/// Cases:
///   1. COUNT(*) on a table with known row count (from Statistics) → literal.
///   2. Aggregate whose group_by is a unique/PK key → the grouping is 1:1,
///      so aggregate functions on other columns can be simplified.
///   3. Aggregate with no aggregate functions (just GROUP BY) → Distinct.
pub struct AggregateElimination;

impl OptimizerRule for AggregateElimination {
    fn name(&self) -> &str { "AggregateElimination" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree. For each Aggregate:
        // 1. If group_by covers a unique key of the input, each group has exactly one row.
        //    → SUM(x) = x, MIN(x) = x, MAX(x) = x, COUNT(*) = 1. Replace with Projection.
        // 2. If aggr_exprs is empty, this is effectively a DISTINCT → leave as-is or
        //    mark for the physical planner.
        todo!("implement AggregateElimination")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_eliminate_aggregate_on_unique_key() { todo!() }

    #[test]
    #[ignore]
    fn test_keep_aggregate_on_non_unique() { todo!() }
}
