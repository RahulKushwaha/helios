// ProjectionElimination rule
//
// ┌──────────────────────────────────────────────────────┐
// │ IMPLEMENTATION ORDER: after PushDownProjection        │
// │ Prerequisites: optimizer/src/optimizer.rs             │
// └──────────────────────────────────────────────────────┘
//
// DataFusion ref: datafusion/optimizer/src/eliminate_projection.rs
// Calcite ref: ProjectRemoveRule

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Removes identity projections that don't transform the schema.
///
/// A projection is an identity if it simply passes through all columns
/// from the input in the same order with no renaming or computation.
///
/// Example:
///   Projection([col("a"), col("b")], Scan{schema: [a, b]})  →  Scan{schema: [a, b]}
///
/// Also merges consecutive projections:
///   Projection(exprs1, Projection(exprs2, input))  →  Projection(composed, input)
pub struct ProjectionElimination;

impl OptimizerRule for ProjectionElimination {
    fn name(&self) -> &str {
        "ProjectionElimination"
    }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Recursively walk the plan tree bottom-up.
        //
        // 1. If the current node is Projection { exprs, input }:
        //    a. Get the input's schema.
        //    b. Check if every expr is Expr::Column(name) and the names match
        //       the input schema fields in the same order.
        //    c. If yes → replace this node with its input (eliminate the projection).
        // 2. Optionally: if input is also a Projection, compose the two expression
        //    lists by substituting column references.
        // 3. Recurse into children first.
        todo!("implement ProjectionElimination")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_eliminate_identity_projection() {
        // Projection([col("a"), col("b")], Scan[a,b]) → Scan[a,b]
        todo!()
    }

    #[test]
    #[ignore]
    fn test_keep_non_identity_projection() {
        // Projection([col("b"), col("a")], Scan[a,b]) → unchanged (reordered)
        todo!()
    }

    #[test]
    #[ignore]
    fn test_keep_computed_projection() {
        // Projection([BinaryExpr(col("a"), Plus, lit(1))], Scan[a]) → unchanged
        todo!()
    }
}
