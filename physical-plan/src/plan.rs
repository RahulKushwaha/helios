// [File 23] PhysicalPlan enum
//
// DataFusion ref: datafusion/physical-plan/src/execution_plan.rs
//
// This is the contract between the optimizer and any execution engine.
// A custom engine only needs to interpret these nodes.

use expr::expr::Expr;
use expr::logical_plan::JoinType;
use expr::schema::Schema;
use std::slice::GetDisjointMutError;

/// A physical execution plan — the concrete operations an engine must perform.
/// Unlike LogicalPlan, this specifies *how* to execute (e.g., NestedLoopJoin vs HashJoin).
#[derive(Debug, Clone, PartialEq)]
pub enum PhysicalPlan {
    /// Scan rows from a named table.
    TableScan { table_name: String, schema: Schema },
    /// Filter rows using a predicate expression.
    Filter {
        predicate: Expr,
        input: Box<PhysicalPlan>,
    },
    /// Project (select) specific expressions from the input.
    Projection {
        exprs: Vec<Expr>,
        input: Box<PhysicalPlan>,
    },
    /// Join two inputs using a nested loop strategy.
    NestedLoopJoin {
        left: Box<PhysicalPlan>,
        right: Box<PhysicalPlan>,
        on: Expr,
        join_type: JoinType,
    },
    /// Sort the input by the given expressions.
    Sort {
        exprs: Vec<Expr>,
        input: Box<PhysicalPlan>,
    },
    /// Hash-based aggregation: group by keys and compute aggregates.
    /// General-purpose. O(group_count) memory. Output unordered.
    HashAggregate {
        group_by: Vec<Expr>,
        aggr_exprs: Vec<Expr>,
        input: Box<PhysicalPlan>,
    },
    /// Sort-based aggregation: assumes input is already sorted on the
    /// group_by columns. Single pass, O(1) extra memory, preserves input
    /// ordering. The only streaming-friendly aggregation strategy.
    SortAggregate {
        group_by: Vec<Expr>,
        aggr_exprs: Vec<Expr>,
        input: Box<PhysicalPlan>,
    },
    /// Aggregation with no GROUP BY: the entire input is one group.
    /// Returns exactly one row, O(1) memory.
    ScalarAggregate {
        aggr_exprs: Vec<Expr>,
        input: Box<PhysicalPlan>,
    },
    Limit {
        skip: usize,
        fetch: usize,
        input: Box<PhysicalPlan>,
    },
}
