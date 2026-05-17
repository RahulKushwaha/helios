use std::cmp::Ordering;

use expr::expr::{Expr, Operator};
use expr::schema::Schema;
use expr::types::FieldValue;

/// Evaluate an expression against a single row.
pub fn eval(expr: &Expr, row: &[FieldValue], schema: &Schema) -> Result<FieldValue, String> {
    match expr {
        Expr::Column(name) => {
            let (idx, _) = schema
                .field_by_name(name)
                .ok_or_else(|| format!("column not found: {}", name))?;
            Ok(row[idx].clone())
        }
        Expr::Literal(v) => Ok(v.clone()),
        Expr::BinaryExpr { left, op, right } => {
            let l = eval(left, row, schema)?;
            let r = eval(right, row, schema)?;

            if matches!(l, FieldValue::Null) || matches!(r, FieldValue::Null) {
                return match op {
                    Operator::And => match (&l, &r) {
                        (FieldValue::Bool(false), _) | (_, FieldValue::Bool(false)) => {
                            Ok(FieldValue::Bool(false))
                        }
                        _ => Ok(FieldValue::Null),
                    },
                    Operator::Or => match (&l, &r) {
                        (FieldValue::Bool(true), _) | (_, FieldValue::Bool(true)) => {
                            Ok(FieldValue::Bool(true))
                        }
                        _ => Ok(FieldValue::Null),
                    },
                    _ => Ok(FieldValue::Null),
                };
            }

            match op {
                Operator::Plus => arith(&l, &r, |a, b| a + b, |a, b| a + b),
                Operator::Minus => arith(&l, &r, |a, b| a - b, |a, b| a - b),
                Operator::Multiply => arith(&l, &r, |a, b| a * b, |a, b| a * b),
                Operator::Divide => arith(&l, &r, |a, b| a / b, |a, b| a / b),
                Operator::Eq => Ok(FieldValue::Bool(l == r)),
                Operator::NotEq => Ok(FieldValue::Bool(l != r)),
                Operator::Lt => Ok(FieldValue::Bool(cmp_values(&l, &r) == Ordering::Less)),
                Operator::LtEq => Ok(FieldValue::Bool(cmp_values(&l, &r) != Ordering::Greater)),
                Operator::Gt => Ok(FieldValue::Bool(cmp_values(&l, &r) == Ordering::Greater)),
                Operator::GtEq => Ok(FieldValue::Bool(cmp_values(&l, &r) != Ordering::Less)),
                Operator::And => match (&l, &r) {
                    (FieldValue::Bool(a), FieldValue::Bool(b)) => Ok(FieldValue::Bool(*a && *b)),
                    _ => Err("AND requires booleans".into()),
                },
                Operator::Or => match (&l, &r) {
                    (FieldValue::Bool(a), FieldValue::Bool(b)) => Ok(FieldValue::Bool(*a || *b)),
                    _ => Err("OR requires booleans".into()),
                },
            }
        }
        Expr::AggregateFunction { .. } => {
            Err("aggregate functions cannot be evaluated per-row".into())
        }
    }
}

pub fn arith(
    l: &FieldValue,
    r: &FieldValue,
    int_op: fn(i64, i64) -> i64,
    float_op: fn(f64, f64) -> f64,
) -> Result<FieldValue, String> {
    match (l, r) {
        (FieldValue::Int(a), FieldValue::Int(b)) => Ok(FieldValue::Int(int_op(*a, *b))),
        (FieldValue::Float(a), FieldValue::Float(b)) => Ok(FieldValue::Float(float_op(*a, *b))),
        (FieldValue::Int(a), FieldValue::Float(b)) => Ok(FieldValue::Float(float_op(*a as f64, *b))),
        (FieldValue::Float(a), FieldValue::Int(b)) => Ok(FieldValue::Float(float_op(*a, *b as f64))),
        _ => Err(format!("cannot do arithmetic on {:?} and {:?}", l, r)),
    }
}

pub fn cmp_values(a: &FieldValue, b: &FieldValue) -> Ordering {
    match (a, b) {
        (FieldValue::Int(a), FieldValue::Int(b)) => a.cmp(b),
        (FieldValue::Float(a), FieldValue::Float(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
        (FieldValue::Str(a), FieldValue::Str(b)) => a.cmp(b),
        (FieldValue::Bool(a), FieldValue::Bool(b)) => a.cmp(b),
        (FieldValue::Null, FieldValue::Null) => Ordering::Equal,
        (FieldValue::Null, _) => Ordering::Greater,
        (_, FieldValue::Null) => Ordering::Less,
        (FieldValue::Int(a), FieldValue::Float(b)) => {
            (*a as f64).partial_cmp(b).unwrap_or(Ordering::Equal)
        }
        (FieldValue::Float(a), FieldValue::Int(b)) => {
            a.partial_cmp(&(*b as f64)).unwrap_or(Ordering::Equal)
        }
        _ => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use expr::schema::Field;
    use expr::types::DataType;

    fn test_schema() -> Schema {
        Schema::new(vec![
            Field::new("x", DataType::Int),
            Field::new("y", DataType::Int),
        ])
    }

    #[test]
    fn eval_column_ref() {
        let row = vec![FieldValue::Int(10), FieldValue::Int(20)];
        let result = eval(&Expr::Column("x".into()), &row, &test_schema()).unwrap();
        assert_eq!(result, FieldValue::Int(10));
    }

    #[test]
    fn eval_binary_comparison() {
        let row = vec![FieldValue::Int(10), FieldValue::Int(5)];
        let expr = Expr::BinaryExpr {
            left: Box::new(Expr::Column("x".into())),
            op: Operator::Gt,
            right: Box::new(Expr::Literal(FieldValue::Int(5))),
        };
        assert_eq!(eval(&expr, &row, &test_schema()).unwrap(), FieldValue::Bool(true));
    }

    #[test]
    fn eval_arithmetic() {
        let row = vec![FieldValue::Int(3), FieldValue::Int(7)];
        let expr = Expr::BinaryExpr {
            left: Box::new(Expr::Column("x".into())),
            op: Operator::Plus,
            right: Box::new(Expr::Column("y".into())),
        };
        assert_eq!(eval(&expr, &row, &test_schema()).unwrap(), FieldValue::Int(10));
    }

    #[test]
    fn eval_null_propagation() {
        let row = vec![FieldValue::Int(3), FieldValue::Null];
        let expr = Expr::BinaryExpr {
            left: Box::new(Expr::Column("x".into())),
            op: Operator::Plus,
            right: Box::new(Expr::Column("y".into())),
        };
        assert_eq!(eval(&expr, &row, &test_schema()).unwrap(), FieldValue::Null);
    }
}
