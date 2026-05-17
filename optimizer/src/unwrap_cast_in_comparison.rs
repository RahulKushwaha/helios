// UnwrapCastInComparison rule
//
// DataFusion ref: datafusion/optimizer/src/unwrap_cast_in_comparison.rs

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Removes unnecessary casts in comparison predicates.
///
/// Example:
///   CAST(int_col AS Float) = 3.0  →  int_col = 3
///   (if 3.0 can be losslessly represented as Int)
///
/// This enables index usage and filter pushdown that would otherwise
/// be blocked by the cast.
pub struct UnwrapCastInComparison;

impl OptimizerRule for UnwrapCastInComparison {
    fn name(&self) -> &str { "UnwrapCastInComparison" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree. For each BinaryExpr comparison:
        // 1. If one side is Cast(col, target_type) and the other is Literal:
        //    a. Try to cast the literal to the column's original type.
        //    b. If the cast is lossless (e.g., 3.0 → 3), remove the Cast
        //       and replace the literal with the casted value.
        //    c. If lossy (e.g., 3.14 → 3), leave as-is.
        // 2. Requires Expr::Cast variant (add if not present).
        todo!("implement UnwrapCastInComparison")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_unwrap_lossless_cast() { todo!() }

    #[test]
    #[ignore]
    fn test_keep_lossy_cast() { todo!() }
}
