// Decorrelation rule
//
// DataFusion ref: datafusion/optimizer/src/decorrelate.rs, decorrelate_predicate_subquery.rs
// Calcite ref: RelDecorrelator

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Rewrites correlated subqueries into joins.
///
/// A correlated subquery references columns from an outer query:
///   SELECT * FROM a WHERE a.x IN (SELECT b.x FROM b WHERE b.id = a.id)
///                                                          ^^^^ outer ref
///
/// This is rewritten as:
///   SemiJoin(a, b, a.id = b.id AND a.x = b.x)
///
/// Without decorrelation the engine must re-execute the subquery for every
/// outer row — O(N × M). After decorrelation it becomes a single join — O(N + M).
///
/// Note: Requires adding subquery-related variants to LogicalPlan and Expr.
/// Suggested additions:
///   LogicalPlan::Subquery { input, outer_refs }
///   LogicalPlan::SemiJoin { left, right, on }
///   LogicalPlan::AntiJoin { left, right, on }
///   Expr::OuterColumnRef(String)  — reference to a column from the outer scope
pub struct Decorrelation;

impl OptimizerRule for Decorrelation {
    fn name(&self) -> &str { "Decorrelation" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree. For each node containing a correlated subquery:
        //
        // 1. Identify outer column references (Expr::OuterColumnRef) in the subquery.
        // 2. Extract the correlation predicates (those referencing outer columns).
        // 3. Rewrite the subquery as a join:
        //    a. EXISTS subquery → SemiJoin (keep outer row if any match)
        //    b. NOT EXISTS       → AntiJoin (keep outer row if no match)
        //    c. IN subquery      → SemiJoin with equality on the IN column
        //    d. Scalar subquery  → LeftJoin (see ScalarSubqueryFlattening)
        // 4. Move correlation predicates into the join condition.
        // 5. Replace OuterColumnRef with regular Column refs (now in same scope).
        todo!("implement Decorrelation")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_exists_to_semi_join() { todo!() }

    #[test]
    #[ignore]
    fn test_not_exists_to_anti_join() { todo!() }

    #[test]
    #[ignore]
    fn test_no_change_uncorrelated() { todo!() }
}
