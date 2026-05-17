// PropagateEmptyRelation rule
//
// DataFusion ref: datafusion/optimizer/src/propagate_empty_relation.rs

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Prunes subtrees that provably produce zero rows.
///
/// Cases:
///   1. Filter with predicate `false` → empty relation.
///   2. Inner Join where either side is empty → empty relation.
///   3. Projection/Sort/Filter over empty → empty.
///   4. Left Join where left is empty → empty.
///   5. Aggregate over empty with no GROUP BY → single row of defaults (COUNT=0, etc.)
///
/// Note: You may need to add a LogicalPlan::EmptyRelation { schema } variant.
pub struct PropagateEmptyRelation;

impl OptimizerRule for PropagateEmptyRelation {
    fn name(&self) -> &str { "PropagateEmptyRelation" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree bottom-up.
        // 1. Mark nodes as "empty" if they provably produce zero rows.
        // 2. Propagate emptiness upward:
        //    - Filter/Projection/Sort over empty → empty.
        //    - Inner Join with any empty child → empty.
        //    - Left Join with empty left → empty.
        //    - Left Join with empty right → left input (no matches, all NULLs on right).
        // 3. Replace empty subtrees with EmptyRelation { schema }.
        todo!("implement PropagateEmptyRelation")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_propagate_through_filter() { todo!() }

    #[test]
    #[ignore]
    fn test_inner_join_with_empty_side() { todo!() }
}
