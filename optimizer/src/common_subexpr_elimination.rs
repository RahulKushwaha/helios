// CommonSubexprElimination (CSE) rule
//
// DataFusion ref: datafusion/optimizer/src/common_subexpr_eliminate.rs
// Calcite ref: RelDecorrelator (uses CSE internally)

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Identifies and eliminates duplicate sub-expressions.
///
/// Example:
///   Projection([a + b, (a + b) * 2], input)
///     → let _cse_0 = a + b; Projection([_cse_0, _cse_0 * 2], input)
///
/// In practice this means computing `a + b` once and reusing the result.
/// Implementation approach: add a Projection below that computes shared
/// sub-expressions, then rewrite the upper expressions to reference them.
pub struct CommonSubexprElimination;

impl OptimizerRule for CommonSubexprElimination {
    fn name(&self) -> &str { "CommonSubexprElimination" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree. For each node with expressions:
        // 1. Collect all sub-expressions and count occurrences.
        // 2. For any sub-expression appearing more than once:
        //    a. Create a synthetic column name (e.g., `__cse_0`).
        //    b. Add a Projection below that computes it.
        //    c. Replace all occurrences with Column("__cse_0").
        // 3. Recurse into children first.
        todo!("implement CommonSubexprElimination")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_eliminate_duplicate_expr() { todo!() }

    #[test]
    #[ignore]
    fn test_no_change_unique_exprs() { todo!() }
}
