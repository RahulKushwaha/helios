// [File 18] PushDownProjection rule
//
// ┌──────────────────────────────────────────────────────┐
// │ IMPLEMENTATION ORDER: 9 of 15                        │
// │ Prerequisites: optimizer/src/optimizer.rs (step 6)   │
// │ Next: optimizer/src/join_reorder.rs (step 10)        │
// └──────────────────────────────────────────────────────┘
//
// DataFusion ref: datafusion/optimizer/src/optimize_projections/ (similar concept)

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Pushes projections down to eliminate unnecessary columns early,
/// reducing the amount of data flowing through the plan.
///
/// Example: if a query only uses columns x and y from a 10-column table,
/// insert a narrow projection just above the Scan to only read x and y.
pub struct PushDownProjection;

impl OptimizerRule for PushDownProjection {
    fn name(&self) -> &str {
        "PushDownProjection"
    }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan top-down, collecting the set of columns that are
        // actually needed by ancestor nodes.
        //
        // 1. Start from the root Projection (or all columns if no projection).
        // 2. For each node, determine which columns it needs from its children:
        //    - Filter: needs columns in the predicate + columns needed by parent
        //    - Join: needs columns in the join condition + columns needed by parent
        //    - Aggregate: needs group_by columns + columns in aggregate expressions
        //    - Sort: needs sort expression columns + columns needed by parent
        // 3. At Scan nodes, insert a Projection that only includes the required columns
        //    (if fewer than the full schema).
        //
        // Hint: write a helper `fn collect_required_columns(plan: &LogicalPlan, needed: &mut HashSet<String>)`
        // that recursively determines which columns each node requires.
        todo!("implement projection pushdown")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prune_unused_columns() {
        // TODO: Build Projection([x, y], Scan(t1 with columns x, y, z, w, v)),
        // run PushDownProjection, verify a narrow projection is inserted above the scan
        todo!()
    }

    #[test]
    fn test_preserve_filter_columns() {
        // TODO: Build Projection([x], Filter(y > 5, Scan(t1))),
        // verify both x and y are preserved (y is needed by the filter)
        todo!()
    }

    #[test]
    fn test_preserve_join_columns() {
        // TODO: Build Projection([x], Join(on: t1.y = t2.y, ...)),
        // verify y is preserved on both sides (needed by join condition)
        todo!()
    }
}
