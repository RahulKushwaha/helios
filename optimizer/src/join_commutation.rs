// JoinCommutation rule
//
// Calcite ref: JoinCommuteRule

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Swaps left and right inputs of a join based on cost heuristics.
///
/// For hash joins the smaller table should be the build side (right).
/// For nested-loop joins the smaller table should be the inner (right).
pub struct JoinCommutation;

impl OptimizerRule for JoinCommutation {
    fn name(&self) -> &str { "JoinCommutation" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree. For each Inner Join:
        // 1. Estimate row counts for left and right (via Statistics).
        // 2. If left is smaller than right, swap them.
        // 3. Adjust the join condition columns accordingly.
        // 4. For Left/Right joins, commutation flips the join type too.
        todo!("implement JoinCommutation")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_swap_when_left_smaller() { todo!() }

    #[test]
    #[ignore]
    fn test_no_swap_when_left_larger() { todo!() }
}
