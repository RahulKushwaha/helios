// [File 19] JoinReorder rule
//
// ┌──────────────────────────────────────────────────────┐
// │ IMPLEMENTATION ORDER: 10 of 15                       │
// │ Prerequisites: optimizer/src/optimizer.rs (step 6),  │
// │                expr/src/statistics.rs                 │
// │ Next: optimizer/src/cost_model.rs (step 11)          │
// └──────────────────────────────────────────────────────┘
//
// DataFusion ref: datafusion/optimizer/src/join_key_set.rs (related concept)

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;
use expr::statistics::Statistics;
use std::collections::HashMap;

/// Reorders a chain of inner joins to minimize intermediate result sizes.
///
/// Uses a greedy heuristic: pick the smallest table first (by row_count from Statistics),
/// then greedily add the next table that produces the cheapest join.
pub struct JoinReorder {
    /// Table name → statistics, used to estimate table sizes for reordering.
    pub table_stats: HashMap<String, Statistics>,
}

impl JoinReorder {
    pub fn new(table_stats: HashMap<String, Statistics>) -> Self {
        Self { table_stats }
    }
}

impl OptimizerRule for JoinReorder {
    fn name(&self) -> &str {
        "JoinReorder"
    }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Detect chains of inner joins in the plan tree.
        //
        // 1. Flatten: walk the plan and collect all base tables (Scan nodes) and
        //    join predicates from a chain of consecutive Inner Join nodes.
        //    Stop flattening when you hit a non-inner-join node.
        //
        // 2. Reorder: sort base tables by row_count (ascending) from self.table_stats.
        //    Use a greedy algorithm:
        //    - Start with the smallest table.
        //    - Repeatedly pick the next table that has a join predicate connecting it
        //      to the already-joined set, preferring the smallest.
        //
        // 3. Rebuild: reconstruct the join tree in the new order, re-attaching
        //    the appropriate join predicates.
        //
        // 4. Recurse into non-join children (Filter, Projection, etc.).
        //
        // Hint: write helpers:
        //   `fn flatten_joins(plan: LogicalPlan) -> (Vec<LogicalPlan>, Vec<Expr>)`
        //   `fn rebuild_joins(tables: Vec<LogicalPlan>, predicates: Vec<Expr>) -> LogicalPlan`
        todo!("implement join reordering with greedy heuristic")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reorder_three_tables() {
        // TODO: Build a left-deep join of t1(1000 rows), t2(10 rows), t3(100 rows),
        // run JoinReorder, verify the new order is t2 ⋈ t3 ⋈ t1 (smallest first)
        todo!()
    }

    #[test]
    fn test_preserve_join_predicates() {
        // TODO: After reordering, verify all original join predicates are still present
        todo!()
    }

    #[test]
    fn test_no_reorder_single_join() {
        // TODO: A plan with only one join should be returned unchanged
        todo!()
    }
}
