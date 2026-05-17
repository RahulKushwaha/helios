// [File 24] PhysicalPlanner — converts LogicalPlan → PhysicalPlan
//
// ┌──────────────────────────────────────────────────────┐
// │ IMPLEMENTATION ORDER: 12 of 15                       │
// │ Prerequisites: physical-plan/src/plan.rs,            │
// │                expr/src/logical_plan/plan.rs (step 4)│
// │ Next: execution/src/evaluator.rs (step 13)           │
// └──────────────────────────────────────────────────────┘
//
// DataFusion ref: datafusion/core/src/physical_planner.rs

use crate::plan::PhysicalPlan;
use expr::logical_plan::LogicalPlan;
use std::fmt;

#[derive(Debug)]
pub enum PlannerError {
    UnsupportedPlan(String),
    Internal(String),
}

impl fmt::Display for PlannerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlan(msg) => write!(f, "Unsupported plan: {msg}"),
            Self::Internal(msg) => write!(f, "Planner error: {msg}"),
        }
    }
}

/// Converts an optimized LogicalPlan into a PhysicalPlan that an execution engine can run.
pub struct PhysicalPlanner;

impl PhysicalPlanner {
    pub fn new() -> Self {
        Self
    }

    /// Convert a LogicalPlan tree into a PhysicalPlan tree.
    pub fn create_physical_plan(
        &self,
        logical_plan: &LogicalPlan,
    ) -> Result<PhysicalPlan, PlannerError> {
        // TODO: Recursively map each LogicalPlan variant to its PhysicalPlan counterpart:
        //
        //   LogicalPlan::Scan       → PhysicalPlan::TableScan
        //   LogicalPlan::Filter     → PhysicalPlan::Filter
        //   LogicalPlan::Projection → PhysicalPlan::Projection
        //   LogicalPlan::Join       → PhysicalPlan::NestedLoopJoin
        //   LogicalPlan::Sort       → PhysicalPlan::Sort
        //   LogicalPlan::Aggregate  → PhysicalPlan::HashAggregate
        //
        // For each node, recursively convert child plans first, then wrap in
        // the corresponding physical node. Clone expressions as-is (physical
        // plan reuses the same Expr types for now).
        let plan = match logical_plan {
            LogicalPlan::Scan { table_name, schema } => PhysicalPlan::TableScan {
                table_name: table_name.to_string(),
                schema: schema.clone(),
            },
            LogicalPlan::Filter { input, predicate } => PhysicalPlan::Filter {
                predicate: predicate.clone(),
                input: Box::new(self.create_physical_plan(input)?),
            },
            LogicalPlan::Projection { exprs, input } => PhysicalPlan::Projection {
                exprs: exprs.clone(),
                input: Box::new(self.create_physical_plan(input)?),
            },
            LogicalPlan::Join {
                left,
                right,
                on,
                join_type,
            } => PhysicalPlan::NestedLoopJoin {
                left: Box::new(self.create_physical_plan(left)?),
                right: Box::new(self.create_physical_plan(right)?),
                on: on.clone(),
                join_type: join_type.clone(),
            },
            LogicalPlan::Sort { exprs, input } => PhysicalPlan::Sort {
                exprs: exprs.clone(),
                input: Box::new(self.create_physical_plan(input)?),
            },
            LogicalPlan::Aggregate {
                group_by,
                aggr_exprs,
                input,
            } => PhysicalPlan::HashAggregate {
                group_by: group_by.clone(),
                aggr_exprs: aggr_exprs.clone(),
                input: Box::new(self.create_physical_plan(input)?),
            },
            LogicalPlan::Limit { skip, fetch, input } => PhysicalPlan::Limit {
                skip: *skip,
                fetch: *fetch,
                input: Box::new(self.create_physical_plan(input)?),
            },
        };

        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_plan_simple_scan() {
        // TODO: Create a LogicalPlan::Scan, convert it, verify result is PhysicalPlan::TableScan
        todo!()
    }

    #[test]
    fn test_plan_join_to_nested_loop() {
        // TODO: Create a LogicalPlan::Join, convert it,
        // verify result is PhysicalPlan::NestedLoopJoin
        todo!()
    }

    #[test]
    fn test_plan_aggregate_to_hash() {
        // TODO: Create a LogicalPlan::Aggregate, convert it,
        // verify result is PhysicalPlan::HashAggregate
        todo!()
    }
}
