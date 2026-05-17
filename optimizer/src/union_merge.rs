// UnionMerge rule
//
// Calcite ref: UnionMergeRule

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Merges nested Union nodes and eliminates redundant ones.
///
/// Cases:
///   1. Union(Union(a, b), c) → Union(a, b, c)  (flatten)
///   2. Union(a) with single input → a  (eliminate)
///   3. Union(a, a) → Union(a) if dedup is desired (depends on UNION vs UNION ALL)
///
/// Note: Our LogicalPlan doesn't have a Union variant yet. This skeleton
/// is ready for when you add one.
pub struct UnionMerge;

impl OptimizerRule for UnionMerge {
    fn name(&self) -> &str { "UnionMerge" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree. For each Union node:
        // 1. Collect all children recursively (flatten nested Unions).
        // 2. If only one child remains, replace Union with that child.
        // 3. Recurse into children first.
        todo!("implement UnionMerge")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_flatten_nested_unions() { todo!() }

    #[test]
    #[ignore]
    fn test_eliminate_single_input_union() { todo!() }
}
