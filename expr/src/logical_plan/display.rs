// [File 11] Pretty-print for LogicalPlan
//
// ┌─────────────────────────────────────────────────────┐
// │ IMPLEMENTATION ORDER: 5 of 15                       │
// │ Prerequisites: expr/src/logical_plan/plan.rs (4),   │
// │                expr/src/expr.rs (step 3)            │
// │ Next: optimizer/src/optimizer.rs (step 6)           │
// └─────────────────────────────────────────────────────┘
//
// DataFusion ref: datafusion/expr/src/logical_plan/display.rs

use crate::logical_plan::plan::LogicalPlan;
use std::fmt;

impl fmt::Display for LogicalPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO: Print the plan as an indented tree, e.g.:
        //
        //   Projection: x, y
        //     Filter: x > 5
        //       Scan: t1
        //
        // Hint: write a recursive helper fn that takes an indent level,
        // prints the current node, then recurses into children with indent + 1.
        // Use write!(f, "{:indent$}{}", "", node_description, indent = depth * 2)
        todo!("implement Display for LogicalPlan as indented tree")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{col, lit};
    use crate::logical_plan::builder::LogicalPlanBuilder;
    use crate::schema::{Field, Schema};
    use crate::types::{DataType, FieldValue};

    #[test]
    fn test_display_plan_tree() {
        // TODO: Build a multi-level plan, call format!("{}", plan),
        // verify the output string contains expected indented node names
        todo!()
    }
}
