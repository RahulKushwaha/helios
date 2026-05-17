// JoinTypeConversion rule
//
// DataFusion ref: datafusion/optimizer/src/eliminate_outer_join.rs
// Calcite ref: JoinPushThroughJoinRule (related)

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Converts outer joins to inner joins when the nullable side is filtered.
///
/// Example:
///   Filter(b.x IS NOT NULL, LeftJoin(a, b, on))
///     → Filter(b.x IS NOT NULL, InnerJoin(a, b, on))
///
/// A LEFT JOIN produces NULLs for the right side when there's no match.
/// If a filter above rejects NULLs on the right side, those NULL rows
/// would be eliminated anyway — so the join can safely become INNER.
pub struct JoinTypeConversion;

impl OptimizerRule for JoinTypeConversion {
    fn name(&self) -> &str {
        "JoinTypeConversion"
    }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Recursively walk the plan tree.
        //
        // 1. If the current node is Filter { predicate, input }
        //    and input is Join { join_type: Left|Right|Full, .. }:
        //    a. Determine which side of the join is "nullable" (right for Left, left for Right).
        //    b. Check if the predicate rejects NULLs on the nullable side
        //       (e.g., references a column from that side in a comparison).
        //    c. If yes → change join_type to Inner.
        //    d. For Full → if predicate rejects NULLs on right → Left;
        //       if on left → Right; if both → Inner.
        // 2. Recurse into children first.
        todo!("implement JoinTypeConversion")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_left_to_inner() {
        // Filter(b.x > 5, LeftJoin(a, b)) → Filter(b.x > 5, InnerJoin(a, b))
        todo!()
    }

    #[test]
    #[ignore]
    fn test_no_conversion_without_null_rejection() {
        // Filter on left-side column only → LeftJoin stays
        todo!()
    }
}
