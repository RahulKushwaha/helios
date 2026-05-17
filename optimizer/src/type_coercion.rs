// TypeCoercion rule
//
// DataFusion ref: datafusion/optimizer/src/analyzer/type_coercion.rs

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Inserts implicit type casts to make expressions type-safe.
///
/// Examples:
///   `int_col = 3.14`  → `CAST(int_col AS Float) = 3.14`
///   `int_col + float_col` → `CAST(int_col AS Float) + float_col`
///
/// Promotion rules (simplified):
///   Int + Float → Float
///   Bool in arithmetic → Int
///   Any comparison with Str → both cast to Str
pub struct TypeCoercion;

impl OptimizerRule for TypeCoercion {
    fn name(&self) -> &str { "TypeCoercion" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree. For each node with expressions:
        // 1. For each BinaryExpr, resolve the types of left and right.
        // 2. If types differ, determine the common type using promotion rules.
        // 3. Wrap the narrower side in a Cast expression (you may need to add
        //    Expr::Cast { expr, to_type } to the Expr enum first).
        // 4. For AggregateFunction args, verify types match the function's expectations.
        todo!("implement TypeCoercion")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_coerce_int_float_comparison() { todo!() }

    #[test]
    #[ignore]
    fn test_no_coercion_same_types() { todo!() }
}
