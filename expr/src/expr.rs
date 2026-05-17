// [File 07] Expression types
//
// ┌─────────────────────────────────────────────────────┐
// │ IMPLEMENTATION ORDER: 3 of 15                       │
// │ Prerequisites: expr/src/types.rs (step 1)           │
// │ Next: expr/src/logical_plan/plan.rs (step 4)        │
// └─────────────────────────────────────────────────────┘
//
// DataFusion ref: datafusion/expr/src/expr.rs

use crate::types::FieldValue;
use std::fmt;

/// Comparison and arithmetic operators.
#[derive(Debug, Clone, PartialEq)]
pub enum Operator {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Plus,
    Minus,
    Multiply,
    Divide,
    And,
    Or,
}

/// Aggregate function types.
#[derive(Debug, Clone, PartialEq)]
pub enum AggFunc {
    Count,
    Sum,
    Min,
    Max,
    Avg,
}

impl fmt::Display for AggFunc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AggFunc::Count => {
                write!(f, "Count")
            }
            AggFunc::Sum => {
                write!(f, "Sum")
            }
            AggFunc::Min => {
                write!(f, "Min")
            }
            AggFunc::Max => {
                write!(f, "Max")
            }
            AggFunc::Avg => {
                write!(f, "Avg")
            }
        }
    }
}

/// An expression in the query plan — can appear in filters, projections, join conditions, etc.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Reference to a column by name.
    Column(String),
    /// A constant value.
    Literal(FieldValue),
    /// A binary operation: left op right.
    BinaryExpr {
        left: Box<Expr>,
        op: Operator,
        right: Box<Expr>,
    },
    /// An aggregate function call.
    AggregateFunction { fun: AggFunc, args: Vec<Expr> },
}

/// Helper to construct a column reference.
pub fn col(name: impl Into<String>) -> Expr {
    Expr::Column(name.into())
}

/// Helper to construct a literal value.
pub fn lit(value: FieldValue) -> Expr {
    Expr::Literal(value)
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO: Pretty-print expressions, e.g.:
        //   Column("x") -> "x"
        //   Literal(Int(5)) -> "5"
        //   BinaryExpr { left: col("x"), op: Gt, right: lit(5) } -> "x > 5"
        //   AggregateFunction { fun: Sum, args: [col("x")] } -> "SUM(x)"
        // todo!("implement Display for Expr")
        match self {
            Expr::Column(column) => write!(f, "Column(\"{}\")", column),
            Expr::Literal(literal) => write!(f, "Literal(\"{}\")", literal),
            Expr::BinaryExpr { left, op, right } => {
                write!(
                    f,
                    "BinaryExpr (left: {}, op: {}, right: {})",
                    left.to_string(),
                    op.to_string(),
                    right.to_string()
                )
            }
            Expr::AggregateFunction { fun, args } => {
                write!(f, "Aggregate {}(", fun)?;
                for arg in args {
                    write!(f, " {}", arg)?
                }
                Ok(())
            }
        }
    }
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO: Map each operator to its symbol string:
        //   Eq -> "=", NotEq -> "!=", Lt -> "<", And -> "AND", Plus -> "+", etc.
        // todo!("implement Display for Operator")
        match self {
            Operator::Eq => {
                write!(f, "=")
            }
            Operator::NotEq => {
                write!(f, "!=")
            }
            Operator::Lt => {
                write!(f, "<")
            }
            Operator::LtEq => {
                write!(f, "<=")
            }
            Operator::Gt => {
                write!(f, ">")
            }
            Operator::GtEq => {
                write!(f, ">=")
            }
            Operator::Plus => {
                write!(f, "+")
            }
            Operator::Minus => {
                write!(f, "-")
            }
            Operator::Multiply => {
                write!(f, "*")
            }
            Operator::Divide => {
                write!(f, "/")
            }
            Operator::And => {
                write!(f, "&&")
            }
            Operator::Or => {
                write!(f, "||")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── col / lit helpers ──────────────────────────────

    #[test]
    fn col_creates_column() {
        assert_eq!(col("x"), Expr::Column("x".into()));
    }

    #[test]
    fn lit_creates_literal() {
        assert_eq!(lit(FieldValue::Int(5)), Expr::Literal(FieldValue::Int(5)));
    }

    // ── BinaryExpr construction ────────────────────────

    #[test]
    fn binary_expr_structure() {
        let e = Expr::BinaryExpr {
            left: Box::new(col("a")),
            op: Operator::Gt,
            right: Box::new(lit(FieldValue::Int(10))),
        };
        match e {
            Expr::BinaryExpr { left, op, right } => {
                assert_eq!(*left, col("a"));
                assert_eq!(op, Operator::Gt);
                assert_eq!(*right, lit(FieldValue::Int(10)));
            }
            _ => panic!("expected BinaryExpr"),
        }
    }

    // ── AggregateFunction construction ─────────────────

    #[test]
    fn aggregate_function_structure() {
        let e = Expr::AggregateFunction {
            fun: AggFunc::Sum,
            args: vec![col("price")],
        };
        match e {
            Expr::AggregateFunction { fun, args } => {
                assert_eq!(fun, AggFunc::Sum);
                assert_eq!(args, vec![col("price")]);
            }
            _ => panic!("expected AggregateFunction"),
        }
    }

    // ── Operator Display ───────────────────────────────

    #[test]
    fn display_comparison_ops() {
        assert_eq!(Operator::Eq.to_string(), "=");
        assert_eq!(Operator::NotEq.to_string(), "!=");
        assert_eq!(Operator::Lt.to_string(), "<");
        assert_eq!(Operator::LtEq.to_string(), "<=");
        assert_eq!(Operator::Gt.to_string(), ">");
        assert_eq!(Operator::GtEq.to_string(), ">=");
    }

    #[test]
    fn display_arithmetic_ops() {
        assert_eq!(Operator::Plus.to_string(), "+");
        assert_eq!(Operator::Minus.to_string(), "-");
        assert_eq!(Operator::Multiply.to_string(), "*");
        assert_eq!(Operator::Divide.to_string(), "/");
    }

    #[test]
    fn display_logical_ops() {
        assert_eq!(Operator::And.to_string(), "&&");
        assert_eq!(Operator::Or.to_string(), "||");
    }

    // ── AggFunc Display ────────────────────────────────

    #[test]
    fn display_agg_funcs() {
        assert_eq!(AggFunc::Count.to_string(), "Count");
        assert_eq!(AggFunc::Sum.to_string(), "Sum");
        assert_eq!(AggFunc::Min.to_string(), "Min");
        assert_eq!(AggFunc::Max.to_string(), "Max");
        assert_eq!(AggFunc::Avg.to_string(), "Avg");
    }

    // ── Expr Display ───────────────────────────────────

    #[test]
    fn display_column() {
        assert_eq!(col("x").to_string(), "x");
    }

    #[test]
    fn display_literal() {
        assert_eq!(lit(FieldValue::Int(42)).to_string(), "42");
        assert_eq!(lit(FieldValue::Null).to_string(), "null");
    }

    #[test]
    fn display_binary_expr() {
        let e = Expr::BinaryExpr {
            left: Box::new(col("x")),
            op: Operator::Gt,
            right: Box::new(lit(FieldValue::Int(5))),
        };
        let s = e.to_string();
        // Should contain the column, operator, and literal
        assert!(s.contains("x"));
        assert!(s.contains(">"));
        assert!(s.contains("5"));
    }

    #[test]
    fn display_aggregate() {
        let e = Expr::AggregateFunction {
            fun: AggFunc::Sum,
            args: vec![col("price")],
        };
        let s = e.to_string();
        assert!(s.contains("Sum"));
        assert!(s.contains("price"));
    }

    // ── Clone / Eq ─────────────────────────────────────

    #[test]
    fn expr_clone_eq() {
        let e = col("a");
        assert_eq!(e.clone(), e);
    }

    #[test]
    fn expr_ne() {
        assert_ne!(col("a"), col("b"));
        assert_ne!(col("a"), lit(FieldValue::Str("a".into())));
    }
}
