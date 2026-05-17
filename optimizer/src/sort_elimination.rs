// SortElimination rule
//
// ┌──────────────────────────────────────────────────────┐
// │ IMPLEMENTATION ORDER: last among the new rules        │
// │ Prerequisites: optimizer/src/optimizer.rs             │
// └──────────────────────────────────────────────────────┘
//
// DataFusion ref: datafusion/optimizer/src/eliminate_sort.rs (related)
// Calcite ref: SortRemoveRule
//
// Introduces the concept of "interesting orders" — tracking whether the
// input is already sorted in the required order so the Sort can be removed.

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Removes redundant Sort nodes when the input is already ordered.
///
/// Cases where a Sort can be eliminated:
///   1. Sort over another Sort with the same expressions → remove outer Sort.
///   2. Sort over a single-row input (e.g., Aggregate with no GROUP BY) → remove Sort.
///   3. Sort whose expressions are a prefix of the input's existing order → remove Sort.
///
/// Example:
///   Sort([col("a")], Sort([col("a"), col("b")], input))
///     → Sort([col("a"), col("b")], input)
///   (the inner sort already satisfies the outer's requirement)
pub struct SortElimination;

impl OptimizerRule for SortElimination {
    fn name(&self) -> &str {
        "SortElimination"
    }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Recursively walk the plan tree bottom-up.
        //
        // 1. If the current node is Sort { exprs: outer_exprs, input }
        //    and input is Sort { exprs: inner_exprs, input: inner }:
        //    a. If outer_exprs == inner_exprs → eliminate the outer Sort, keep inner.
        //    b. If outer_exprs is a prefix of inner_exprs → keep only the inner Sort
        //       (it already satisfies the outer requirement).
        //    c. Otherwise → keep the outer Sort but remove the inner (it will be
        //       re-sorted anyway).
        // 2. If the current node is Sort and the input is an Aggregate with
        //    an empty group_by → the result is a single row, Sort is a no-op, remove it.
        // 3. Recurse into children first.
        todo!("implement SortElimination")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_eliminate_duplicate_sort() {
        // Sort(a, Sort(a, input)) → Sort(a, input)
        todo!()
    }

    #[test]
    #[ignore]
    fn test_eliminate_sort_over_single_row_aggregate() {
        // Sort(a, Aggregate([], [count(*)], input)) → Aggregate([], [count(*)], input)
        todo!()
    }

    #[test]
    #[ignore]
    fn test_keep_sort_different_exprs() {
        // Sort(a, Sort(b, input)) → Sort(a, input) — inner sort is wasted
        todo!()
    }
}
