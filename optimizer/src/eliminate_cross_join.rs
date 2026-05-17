// EliminateCrossJoin rule
//
// ┌──────────────────────────────────────────────────────┐
// │ IMPLEMENTATION ORDER: after JoinReorder               │
// │ Prerequisites: optimizer/src/optimizer.rs             │
// └──────────────────────────────────────────────────────┘
//
// DataFusion ref: datafusion/optimizer/src/eliminate_cross_join.rs
// Calcite ref: JoinPushThroughJoinRule (related concept)

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Converts a cross join followed by a filter into an inner join.
///
/// A cross join is a Join with a `true` literal as the condition (or no real condition).
/// When a Filter above it references columns from both sides, the filter predicate
/// can become the join condition.
///
/// Example:
///   Filter(a.id = b.id, Join(left, right, true, Inner))
///     → Join(left, right, a.id = b.id, Inner)
///
/// This is critical for performance — cross joins produce N×M rows before filtering,
/// while an inner join can use the condition to prune during the join.
pub struct EliminateCrossJoin;

impl OptimizerRule for EliminateCrossJoin {
    fn name(&self) -> &str {
        "EliminateCrossJoin"
    }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Recursively walk the plan tree bottom-up.
        //
        // 1. If the current node is Filter { predicate, input }
        //    and input is Join { left, right, on, join_type: Inner }
        //    and `on` is a literal `true` (i.e., a cross join):
        //    a. Check if `predicate` references columns from both left and right schemas.
        //    b. If yes → move predicate into the join condition:
        //       Join { left, right, on: predicate, join_type: Inner }
        //    c. If predicate has mixed parts (some cross-join, some single-table),
        //       split the AND clauses: join-relevant ones become `on`, the rest stay as Filter.
        // 2. Recurse into children first.
        todo!("implement EliminateCrossJoin")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_cross_join_to_inner() {
        // Filter(a.id = b.id, CrossJoin(left, right)) → InnerJoin(left, right, a.id = b.id)
        todo!()
    }

    #[test]
    #[ignore]
    fn test_no_change_when_already_inner_join() {
        // Filter(x > 5, InnerJoin(left, right, a.id = b.id)) → unchanged join, filter stays
        todo!()
    }

    #[test]
    #[ignore]
    fn test_mixed_predicate_split() {
        // Filter(a.id = b.id AND a.x > 5, CrossJoin) → Filter(a.x > 5, InnerJoin(on: a.id = b.id))
        todo!()
    }
}
