// ConstantPropagation rule
//
// Calcite ref: ReduceExpressionsRule (constant propagation phase)

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Propagates known constant values through the plan.
///
/// Example:
///   Filter(x = 5, Projection([x, x + 1], input))
///     → Filter(x = 5, Projection([5, 6], input))
///
/// When a Filter establishes `col = literal`, all references to that column
/// in sibling/descendant expressions can be replaced with the literal.
/// Pairs well with ConstantFolding to then simplify the resulting expressions.
pub struct ConstantPropagation;

impl OptimizerRule for ConstantPropagation {
    fn name(&self) -> &str { "ConstantPropagation" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree top-down (constants flow downward).
        // 1. When encountering Filter with `col = literal` (Eq):
        //    a. Record the binding: column_name → literal_value.
        // 2. In descendant nodes, replace Column(name) with Literal(value)
        //    wherever a binding exists.
        // 3. Handle AND predicates: each `col = lit` clause adds a binding.
        // 4. Be careful with OR: `x = 5 OR y = 3` does NOT let you substitute.
        todo!("implement ConstantPropagation")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_propagate_equality_constant() { todo!() }

    #[test]
    #[ignore]
    fn test_no_propagation_with_or() { todo!() }
}
