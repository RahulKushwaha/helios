// [File 16] ConstantFolding rule
//
// ┌─────────────────────────────────────────────────────┐
// │ IMPLEMENTATION ORDER: 7 of 15                       │
// │ Prerequisites: optimizer/src/optimizer.rs (step 6)  │
// │ Next: optimizer/src/push_down_filter.rs (step 8)    │
// └─────────────────────────────────────────────────────┘
//
// DataFusion ref: datafusion/optimizer/src/simplify_expressions/ (similar concept)

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Evaluates constant sub-expressions at optimization time.
///
/// Examples:
///   `3 + 5`           → `8`
///   `true AND x > 3`  → `x > 3`
///   `false OR expr`   → `expr`
///   `1 = 1`           → `true`
pub struct ConstantFolding;

impl OptimizerRule for ConstantFolding {
    fn name(&self) -> &str {
        "ConstantFolding"
    }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Recursively walk the LogicalPlan tree. For each node that contains
        // expressions (Filter, Projection, Join, Sort, Aggregate), walk the Expr tree
        // and fold constant sub-expressions:
        //
        // 1. If a BinaryExpr has two Literal children, evaluate the operation and
        //    replace with a single Literal.
        // 2. If a BinaryExpr is `true AND expr` or `expr AND true`, simplify to `expr`.
        // 3. If a BinaryExpr is `false OR expr` or `expr OR false`, simplify to `expr`.
        // 4. Recurse into child plan nodes.
        //
        // Hint: write a helper `fn fold_expr(expr: Expr) -> Expr` that handles the
        // expression rewriting, then apply it to each plan node's expressions.
        todo!("implement constant folding over plan tree")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fold_arithmetic() {
        // TODO: Create a Filter with predicate `3 + 5 > x`, run ConstantFolding,
        // verify the predicate becomes `8 > x`
        todo!()
    }

    #[test]
    fn test_fold_boolean_and() {
        // TODO: Create a Filter with predicate `true AND x > 3`, run ConstantFolding,
        // verify the predicate becomes `x > 3`
        todo!()
    }

    #[test]
    fn test_fold_nested() {
        // TODO: Create a nested expression like `(2 + 3) * (1 + 1)`, run ConstantFolding,
        // verify it folds to `10`
        todo!()
    }

    #[test]
    fn test_no_change_when_no_constants() {
        // TODO: Create a Filter with predicate `x > y` (no constants to fold),
        // verify the plan is unchanged after optimization
        todo!()
    }
}
