// [File 17] PushDownFilter (predicate pushdown) rule
//
// ┌─────────────────────────────────────────────────────┐
// │ IMPLEMENTATION ORDER: 8 of 15                       │
// │ Prerequisites: optimizer/src/optimizer.rs (step 6)  │
// │ Next: optimizer/src/push_down_projection.rs (step 9)│
// └─────────────────────────────────────────────────────┘
//
// DataFusion ref: datafusion/optimizer/src/push_down_filter.rs

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Pushes filter predicates closer to the data source to reduce rows early.
///
/// Transformations:
///   Filter above Projection → push filter below projection (if columns available)
///   Filter above Join → split conjuncts, push single-side predicates into join children
pub struct PushDownFilter;

impl OptimizerRule for PushDownFilter {
    fn name(&self) -> &str {
        "PushDownFilter"
    }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Recursively walk the plan tree. When you encounter a Filter node:
        //
        // 1. Filter above Projection:
        //    - Check if all columns referenced in the predicate exist in the
        //      Projection's input schema.
        //    - If yes, swap: Projection(Filter(input)) instead of Filter(Projection(input)).
        //
        // 2. Filter above Join:
        //    - Split the predicate into AND-conjuncts.
        //    - For each conjunct, determine which columns it references.
        //    - If a conjunct only references left-side columns, push it into the left child.
        //    - If a conjunct only references right-side columns, push it into the right child.
        //    - Conjuncts referencing both sides stay above the join.
        //
        // 3. For all other cases, recurse into children.
        //
        // Hint: write helpers:
        //   `fn extract_conjuncts(expr: &Expr) -> Vec<Expr>` — split AND chains
        //   `fn collect_columns(expr: &Expr) -> HashSet<String>` — find referenced columns
        //   `fn combine_conjuncts(exprs: Vec<Expr>) -> Expr` — re-join with AND
        todo!("implement predicate pushdown")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_filter_below_projection() {
        // TODO: Build Filter(Projection(Scan)), run PushDownFilter,
        // verify result is Projection(Filter(Scan))
        todo!()
    }

    #[test]
    fn test_push_filter_into_join_left() {
        // TODO: Build Filter(Join(scan_t1, scan_t2)) where predicate references only t1 columns,
        // verify the filter is pushed into the left child of the join
        todo!()
    }

    #[test]
    fn test_push_filter_into_join_right() {
        // TODO: Same as above but predicate references only t2 (right) columns
        todo!()
    }

    #[test]
    fn test_filter_stays_above_cross_join_predicate() {
        // TODO: Build Filter(Join) where predicate references columns from both sides,
        // verify the filter stays above the join (cannot be pushed down)
        todo!()
    }
}
