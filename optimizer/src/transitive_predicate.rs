// TransitivePredicateInference rule
//
// DataFusion ref: datafusion/optimizer/src/simplify_expressions/
// Calcite ref: ReduceExpressionsRule (transitive closure)

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Infers new predicates from existing equality chains.
///
/// Examples:
///   `a = b AND b = 5`  → also add `a = 5`
///   `a = b AND b = c`  → also add `a = c`
///
/// This enables more aggressive filter pushdown — the inferred predicate
/// `a = 5` can be pushed to the table that owns column `a`.
pub struct TransitivePredicateInference;

impl OptimizerRule for TransitivePredicateInference {
    fn name(&self) -> &str {
        "TransitivePredicateInference"
    }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Recursively walk the plan tree.
        //
        // 1. Collect all equality predicates (Eq) in Filter nodes.
        // 2. Build equivalence classes: if a = b and b = c, then {a, b, c} are equivalent.
        // 3. For each equivalence class, if any member is a Literal, add `col = literal`
        //    predicates for all other column members.
        // 4. AND the new predicates into the existing Filter.
        // 5. Recurse into children.
        todo!("implement TransitivePredicateInference")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_infer_from_equality_chain() {
        // a = b AND b = 5 → a = b AND b = 5 AND a = 5
        todo!()
    }

    #[test]
    #[ignore]
    fn test_no_inference_without_literal() {
        // a = b alone → unchanged (no literal to propagate)
        todo!()
    }
}
