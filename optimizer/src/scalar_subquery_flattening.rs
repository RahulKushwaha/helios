// ScalarSubqueryFlattening rule
//
// DataFusion ref: datafusion/optimizer/src/scalar_subquery_to_join.rs
// Calcite ref: SubQueryRemoveRule (scalar case)

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Flattens scalar subqueries into left joins with aggregation.
///
/// A scalar subquery returns exactly one row and one column, used as a value:
///   SELECT a.*, (SELECT MAX(b.val) FROM b WHERE b.id = a.id) AS max_val FROM a
///
/// Rewritten as:
///   Projection([a.*, sub.max_val],
///     LeftJoin(a,
///       Aggregate(group_by=[b.id], [MAX(b.val) AS max_val], b),
///       a.id = sub.id))
///
/// The left join preserves all outer rows. The aggregate ensures at most
/// one row per group (satisfying the scalar requirement).
pub struct ScalarSubqueryFlattening;

impl OptimizerRule for ScalarSubqueryFlattening {
    fn name(&self) -> &str { "ScalarSubqueryFlattening" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree. For each Projection expression:
        //
        // 1. Check if any expr is a scalar subquery.
        //    (You may need Expr::ScalarSubquery { subquery } variant.)
        // 2. If found:
        //    a. Extract the subquery's LogicalPlan.
        //    b. Identify correlation predicates (outer column refs).
        //    c. Wrap the subquery in an Aggregate grouped by the correlation columns
        //       (if not already aggregated).
        //    d. Create a LeftJoin between the outer input and the aggregated subquery.
        //    e. Replace the scalar subquery expr with a Column ref to the aggregate output.
        // 3. Handle multiple scalar subqueries in the same Projection by chaining joins.
        todo!("implement ScalarSubqueryFlattening")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_scalar_subquery_to_left_join() { todo!() }

    #[test]
    #[ignore]
    fn test_uncorrelated_scalar_subquery() {
        // SELECT (SELECT COUNT(*) FROM b) FROM a → CrossJoin with single-row aggregate
        todo!()
    }

    #[test]
    #[ignore]
    fn test_no_change_without_scalar_subquery() { todo!() }
}
