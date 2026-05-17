// PredicateDecomposition rule
//
// Calcite ref: FilterProjectTransposeRule (related)

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Splits compound AND predicates into separate Filter nodes.
///
/// Example:
///   Filter(a > 5 AND b < 10, input)
///     → Filter(a > 5, Filter(b < 10, input))
///
/// This enables PushDownFilter to push each predicate independently —
/// one might push past a join while the other cannot.
pub struct PredicateDecomposition;

impl OptimizerRule for PredicateDecomposition {
    fn name(&self) -> &str {
        "PredicateDecomposition"
    }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Recursively walk the plan tree.
        //
        // 1. If the current node is Filter { predicate, input }:
        //    a. Flatten the predicate by collecting all AND branches into a Vec<Expr>.
        //    b. If there's more than one, create nested Filter nodes — one per predicate.
        //    c. If there's exactly one, leave as-is.
        // 2. Recurse into children first.
        todo!("implement PredicateDecomposition")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_split_and_predicate() {
        // Filter(a > 5 AND b < 10, Scan) → Filter(a > 5, Filter(b < 10, Scan))
        todo!()
    }

    #[test]
    #[ignore]
    fn test_no_split_single_predicate() {
        // Filter(a > 5, Scan) → unchanged
        todo!()
    }
}
