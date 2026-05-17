// [File 09] LogicalPlan enum
//
// ┌─────────────────────────────────────────────────────┐
// │ IMPLEMENTATION ORDER: 4 of 15                       │
// │ Prerequisites: expr/src/expr.rs (step 3),           │
// │                expr/src/schema.rs (step 2)          │
// │ Next: expr/src/logical_plan/display.rs (step 5)     │
// └─────────────────────────────────────────────────────┘
//
// DataFusion ref: datafusion/expr/src/logical_plan/plan.rs

use crate::expr::{AggFunc, Expr};
use crate::schema::{Field, Schema};
use crate::types::DataType;

/// The type of join operation.
#[derive(Debug, Clone, PartialEq)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
}

/// A logical query plan — a tree of relational operators.
/// Each variant represents one relational algebra operation.
#[derive(Debug, Clone, PartialEq)]
pub enum LogicalPlan {
    /// Read all rows from a named table.
    Scan { table_name: String, schema: Schema },
    /// Keep only rows matching the predicate.
    Filter {
        predicate: Expr,
        input: Box<LogicalPlan>,
    },
    /// Select/compute a list of expressions from the input.
    Projection {
        exprs: Vec<Expr>,
        input: Box<LogicalPlan>,
    },
    /// Combine two inputs on a join condition.
    Join {
        left: Box<LogicalPlan>,
        right: Box<LogicalPlan>,
        on: Expr,
        join_type: JoinType,
    },
    /// Order the input by the given expressions.
    Sort {
        exprs: Vec<Expr>,
        input: Box<LogicalPlan>,
    },
    /// Group the input and compute aggregate expressions.
    Aggregate {
        group_by: Vec<Expr>,
        aggr_exprs: Vec<Expr>,
        input: Box<LogicalPlan>,
    },
    Limit {
        skip: usize,
        fetch: usize,
        input: Box<LogicalPlan>,
    },
}

impl LogicalPlan {
    /// Return the output schema of this plan node.
    pub fn schema(&self) -> Schema {
        match self {
            LogicalPlan::Scan { schema, .. } => schema.clone(),
            LogicalPlan::Filter { input, .. } => input.schema(),
            LogicalPlan::Projection { exprs, input } => Schema::new(
                exprs
                    .iter()
                    .map(|e| expr_to_field(e, &input.schema()))
                    .collect(),
            ),
            LogicalPlan::Join {
                left,
                right,
                join_type,
                ..
            } => {
                let (first, second) = match join_type {
                    JoinType::Right => (right, left),
                    _ => (left, right),
                };
                let mut fields = first.schema().fields;
                fields.extend(second.schema().fields);
                Schema::new(fields)
            }
            LogicalPlan::Sort { input, .. } => input.schema(),
            LogicalPlan::Aggregate {
                group_by,
                aggr_exprs,
                input,
            } => {
                let input_schema = input.schema();
                let fields = group_by
                    .iter()
                    .chain(aggr_exprs.iter())
                    .map(|e| expr_to_field(e, &input_schema))
                    .collect();
                Schema::new(fields)
            }
            LogicalPlan::Limit { skip, fetch, input } => {
                let input_schema = input.schema();
                Schema::new(input_schema.fields)
            }
        }
    }
}

/// Derive a Field from an expression, resolving column types from the input schema.
fn expr_to_field(expr: &Expr, input_schema: &Schema) -> Field {
    match expr {
        Expr::Column(name) => match input_schema.field_by_name(name) {
            Some((_, f)) => f.clone(),
            None => Field::new(name.clone(), DataType::Str),
        },
        Expr::Literal(v) => {
            use crate::types::FieldValue;
            let dt = match v {
                FieldValue::Int(_) => DataType::Int,
                FieldValue::Float(_) => DataType::Float,
                FieldValue::Bool(_) => DataType::Bool,
                FieldValue::Str(_) => DataType::Str,
                FieldValue::Null => DataType::Str,
            };
            Field::new(expr.to_string(), dt)
        }
        Expr::AggregateFunction { fun, args } => {
            let inner_type = args
                .first()
                .map(|a| expr_to_field(a, input_schema).data_type)
                .unwrap_or(DataType::Int);
            let dt = match fun {
                AggFunc::Count => DataType::Int,
                AggFunc::Avg => DataType::Float,
                _ => inner_type,
            };
            Field::new(expr.to_string(), dt)
        }
        Expr::BinaryExpr { left, .. } => Field::new(
            expr.to_string(),
            expr_to_field(left, input_schema).data_type,
        ),
    }
}
