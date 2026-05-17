// SubqueryToJoin rule
//
// DataFusion ref: datafusion/optimizer/src/decorrelate_predicate_subquery.rs
// Calcite ref: SubQueryRemoveRule

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Converts IN / NOT IN subqueries into semi-joins / anti-joins.
///
/// Examples:
///   WHERE x IN (SELECT y FROM b)
///     → SemiJoin(outer, b, outer.x = b.y)
///
///   WHERE x NOT IN (SELECT y FROM b)
///     → AntiJoin(outer, b, outer.x = b.y)
///
/// Unlike Decorrelation (which handles correlated subqueries), this rule
/// handles uncorrelated IN-list subqueries too — the subquery doesn't
/// reference outer columns, but can still be rewritten as a join.
pub struct SubqueryToJoin;

impl OptimizerRule for SubqueryToJoin {
    fn name(&self) -> &str { "SubqueryToJoin" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree. For each Filter node:
        //
        // 1. Check if the predicate contains an IN-subquery expression.
        //    (You may need Expr::InSubquery { expr, subquery, negated } variant.)
        // 2. If found:
        //    a. Extract the subquery's LogicalPlan.
        //    b. Build a join condition: outer_expr = subquery's output column.
        //    c. If negated (NOT IN) → AntiJoin; otherwise → SemiJoin.
        //    d. Replace the Filter + InSubquery with the join node.
        // 3. If the predicate has other parts (AND'd), keep them as a Filter above.
        todo!("implement SubqueryToJoin")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_in_subquery_to_semi_join() { todo!() }

    #[test]
    #[ignore]
    fn test_not_in_to_anti_join() { todo!() }

    #[test]
    #[ignore]
    fn test_no_change_without_subquery() { todo!() }
}
