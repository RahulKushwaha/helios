// DistinctToAggregate rule
//
// DataFusion ref: datafusion/optimizer/src/replace_distinct_aggregate.rs

use crate::optimizer::{OptimizerError, OptimizerRule};
use expr::logical_plan::LogicalPlan;

/// Rewrites DISTINCT operations as GROUP BY aggregations.
///
/// Since our LogicalPlan doesn't have a Distinct variant, this rule
/// detects the pattern: Aggregate with empty aggr_exprs (GROUP BY only)
/// and ensures it's handled efficiently, or converts a future Distinct
/// node if one is added.
///
/// Conceptually: SELECT DISTINCT a, b FROM t → SELECT a, b FROM t GROUP BY a, b
pub struct DistinctToAggregate;

impl OptimizerRule for DistinctToAggregate {
    fn name(&self) -> &str { "DistinctToAggregate" }

    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError> {
        // TODO: Walk the plan tree.
        // 1. If a Distinct node exists (if you add one to LogicalPlan later),
        //    rewrite it as Aggregate { group_by: all_columns, aggr_exprs: [], input }.
        // 2. This unifies the execution path — the engine only needs to handle Aggregate.
        todo!("implement DistinctToAggregate")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_distinct_to_group_by() { todo!() }
}
