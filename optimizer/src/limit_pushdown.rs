// LimitPushdown rule
//
// DataFusion ref: datafusion/optimizer/src/push_down_limit.rs
// Calcite ref: SortUnionTransposeRule (related)

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Pushes LIMIT through projections, unions, and other operators.
///
/// Examples:
///   Limit(10, Projection(exprs, input)) → Projection(exprs, Limit(10, input))
///   Limit(10, Union(a, b)) → Union(Limit(10, a), Limit(10, b))  then Limit(10, ...)
///
/// Note: Our LogicalPlan doesn't have a Limit variant yet. This skeleton
/// is ready for when you add one.
pub struct LimitPushdown;

impl OptimizerRule for LimitPushdown {
    fn name(&self) -> &str { "LimitPushdown" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree. For each Limit node:
        // 1. If child is Projection → push Limit below it.
        // 2. If child is Union → add Limit to each union branch (each branch
        //    only needs to produce N rows), then keep outer Limit.
        // 3. Do NOT push Limit below Filter, Join, or Aggregate (changes semantics).
        todo!("implement LimitPushdown")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_push_limit_through_projection() { todo!() }

    #[test]
    #[ignore]
    fn test_no_push_limit_through_filter() { todo!() }
}
