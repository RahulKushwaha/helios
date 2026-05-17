// NullPropagation rule
//
// DataFusion ref: datafusion/optimizer/src/simplify_expressions/ (handles nulls)

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Propagates NULL through expressions, simplifying where possible.
///
/// Examples:
///   `NULL + x`       → `NULL`
///   `NULL = x`       → `NULL`  (not false!)
///   `NULL AND false`  → `false` (three-valued logic special case)
///   `NULL AND true`   → `NULL`
///   `NULL OR true`    → `true`
///   `NULL OR false`   → `NULL`
pub struct NullPropagation;

impl OptimizerRule for NullPropagation {
    fn name(&self) -> &str { "NullPropagation" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree. For each expression:
        // 1. If a BinaryExpr has a Literal(Null) operand:
        //    a. For arithmetic (+, -, *, /) → result is Null.
        //    b. For comparison (=, !=, <, >, etc.) → result is Null.
        //    c. For AND: Null AND false = false, Null AND true = Null.
        //    d. For OR: Null OR true = true, Null OR false = Null.
        // 2. Replace the expression with Literal(Null) or the simplified result.
        todo!("implement NullPropagation")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_null_arithmetic() { todo!() }

    #[test]
    #[ignore]
    fn test_null_and_three_valued() { todo!() }

    #[test]
    #[ignore]
    fn test_null_or_three_valued() { todo!() }
}
