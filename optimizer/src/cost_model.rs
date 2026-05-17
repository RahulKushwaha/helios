// [File 20] CostModel trait and SimpleCostModel
//
// ┌──────────────────────────────────────────────────────┐
// │ IMPLEMENTATION ORDER: 11 of 15                       │
// │ Prerequisites: expr/src/statistics.rs,               │
// │                expr/src/logical_plan/plan.rs (step 4)│
// │ Next: physical-plan/src/planner.rs (step 12)         │
// └──────────────────────────────────────────────────────┘
//
// DataFusion ref: datafusion/physical-expr/src/analysis.rs (related concept)

use expr::logical_plan::LogicalPlan;
use expr::statistics::Statistics;
use std::collections::HashMap;

/// Trait for estimating the execution cost of a logical plan.
/// Implement this trait to plug in different cost models.
pub trait CostModel {
    /// Estimate the total cost of executing this plan.
    /// Lower cost = better plan.
    fn estimate_cost(
        &self,
        plan: &LogicalPlan,
        table_stats: &HashMap<String, Statistics>,
    ) -> f64;
}

/// A simple cost model using row counts and fixed selectivity estimates.
///
/// Cost formulas:
///   Scan:      row_count (from stats, default 1000 if unknown)
///   Filter:    child_cost × selectivity (default 0.1)
///   Projection: child_cost (no extra cost, just column pruning)
///   Join:      left_rows × right_rows (nested loop estimate)
///   Sort:      child_rows × log2(child_rows)
///   Aggregate: child_rows (hash aggregate, single pass)
pub struct SimpleCostModel;

impl CostModel for SimpleCostModel {
    fn estimate_cost(
        &self,
        plan: &LogicalPlan,
        table_stats: &HashMap<String, Statistics>,
    ) -> f64 {
        // TODO: Recursively compute cost for each plan node using the formulas above.
        //
        // For Scan: look up table_stats[table_name].row_count, default to 1000.0
        // For Filter: recurse into child, multiply by 0.1
        // For Projection: same cost as child
        // For Join: estimate left_rows × right_rows
        //   (you'll need a helper to estimate row count, not just cost)
        // For Sort: child_rows × log2(child_rows)
        // For Aggregate: child_rows
        //
        // Hint: write a helper `fn estimate_row_count(plan, stats) -> f64` alongside
        // the cost estimation, since Join and Sort need row counts from children.
        todo!("implement simple cost model")
    }
}

/// Given a set of candidate plans, pick the one with the lowest estimated cost.
pub fn pick_best_plan(
    candidates: Vec<LogicalPlan>,
    cost_model: &dyn CostModel,
    table_stats: &HashMap<String, Statistics>,
) -> LogicalPlan {
    // TODO: Iterate over candidates, compute cost for each using cost_model,
    // return the plan with the minimum cost.
    // If candidates is empty, panic with a descriptive message.
    todo!("implement best-plan selection")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_cost() {
        // TODO: Create a Scan plan with known stats (e.g., 500 rows),
        // verify SimpleCostModel returns 500.0
        todo!()
    }

    #[test]
    fn test_filter_cost() {
        // TODO: Create Filter(Scan(500 rows)), verify cost is 500.0 * 0.1 = 50.0
        todo!()
    }

    #[test]
    fn test_join_cost() {
        // TODO: Create Join(Scan(100), Scan(200)), verify cost reflects 100 * 200
        todo!()
    }

    #[test]
    fn test_picks_cheaper_plan() {
        // TODO: Create two candidate plans with different costs,
        // verify pick_best_plan returns the cheaper one
        todo!()
    }
}
