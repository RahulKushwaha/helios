// [File 10] LogicalPlanBuilder — ergonomic plan construction
//
// DataFusion ref: datafusion/expr/src/logical_plan/builder.rs

use crate::expr::Expr;
use crate::logical_plan::plan::{JoinType, LogicalPlan};
use crate::schema::Schema;

/// Builder for constructing LogicalPlan trees with a chainable API.
///
/// Usage:
/// ```ignore
/// let plan = LogicalPlanBuilder::scan("t1", schema)
///     .filter(predicate)
///     .project(vec![col("x"), col("y")])
///     .build();
/// ```
pub struct LogicalPlanBuilder {
    plan: LogicalPlan,
}

impl LogicalPlanBuilder {
    /// Start with a table scan.
    pub fn scan(table_name: impl Into<String>, schema: Schema) -> Self {
        Self {
            plan: LogicalPlan::Scan {
                table_name: table_name.into(),
                schema,
            },
        }
    }

    /// Add a filter (WHERE clause) on top of the current plan.
    pub fn filter(self, predicate: Expr) -> Self {
        Self {
            plan: LogicalPlan::Filter {
                predicate,
                input: Box::new(self.plan),
            },
        }
    }

    /// Add a projection (SELECT columns) on top of the current plan.
    pub fn project(self, exprs: Vec<Expr>) -> Self {
        Self {
            plan: LogicalPlan::Projection {
                exprs,
                input: Box::new(self.plan),
            },
        }
    }

    /// Join the current plan with another plan.
    pub fn join(self, right: LogicalPlan, on: Expr, join_type: JoinType) -> Self {
        Self {
            plan: LogicalPlan::Join {
                left: Box::new(self.plan),
                right: Box::new(right),
                on,
                join_type,
            },
        }
    }

    /// Add a sort (ORDER BY) on top of the current plan.
    pub fn sort(self, exprs: Vec<Expr>) -> Self {
        Self {
            plan: LogicalPlan::Sort {
                exprs,
                input: Box::new(self.plan),
            },
        }
    }

    /// Add an aggregate (GROUP BY + aggregate functions) on top of the current plan.
    pub fn aggregate(self, group_by: Vec<Expr>, aggr_exprs: Vec<Expr>) -> Self {
        Self {
            plan: LogicalPlan::Aggregate {
                group_by,
                aggr_exprs,
                input: Box::new(self.plan),
            },
        }
    }

    /// Consume the builder and return the final LogicalPlan.
    pub fn build(self) -> LogicalPlan {
        self.plan
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{col, lit};
    use crate::schema::Field;
    use crate::types::{DataType, FieldValue};

    #[test]
    fn test_build_scan_filter_project() {
        // TODO: Build a plan: scan("t1") -> filter(x > 5) -> project([x, y])
        // Verify the resulting LogicalPlan tree structure matches expectations
        todo!()
    }

    #[test]
    fn test_build_join() {
        // TODO: Build two scans and join them on a condition
        // Verify the Join node has correct left, right, and on fields
        todo!()
    }
}
