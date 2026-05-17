// JoinElimination rule
//
// DataFusion ref: datafusion/optimizer/src/eliminate_join.rs
// Calcite ref: JoinRemoveRule

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Removes joins that provably don't affect the result.
///
/// Cases:
///   1. Inner join on a unique/PK column where the joined table's columns
///      are never used in the output → remove the join.
///   2. Left join where right side columns are never referenced → replace with left input.
pub struct JoinElimination;

impl OptimizerRule for JoinElimination {
    fn name(&self) -> &str { "JoinElimination" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree. For each Join node:
        // 1. Collect column references used by ancestors.
        // 2. If no column from one side is referenced, consider eliminating that side.
        // 3. For Inner: verify join key is unique (needs Statistics) so row count is preserved.
        // 4. For Left: right-side elimination is safe without uniqueness check.
        todo!("implement JoinElimination")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_eliminate_unused_right_side() { todo!() }

    #[test]
    #[ignore]
    fn test_keep_join_when_columns_used() { todo!() }
}
