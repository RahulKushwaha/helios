// [File 05] FieldValue and DataType enums
//
// ┌─────────────────────────────────────────────────────┐
// │ IMPLEMENTATION ORDER: 1 of 15                       │
// │ Prerequisites: None — start here!                   │
// │ Next: expr/src/schema.rs (step 2)                   │
// └─────────────────────────────────────────────────────┘
//
// DataFusion ref: datafusion/common/src/scalar.rs, datafusion/common/src/types.rs

use serde::{Deserialize, Serialize};
use std::fmt;
use std::hash::{Hash, Hasher};

/// Runtime value — the actual data flowing through the query engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldValue {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
}

impl Eq for FieldValue {}

impl Hash for FieldValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            FieldValue::Int(i) => i.hash(state),
            FieldValue::Float(f) => f.to_bits().hash(state),
            FieldValue::Str(s) => s.hash(state),
            FieldValue::Bool(b) => b.hash(state),
            FieldValue::Null => {}
        }
    }
}

/// Column data type — used in schema definitions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DataType {
    Int,
    Float,
    Str,
    Bool,
}

impl fmt::Display for FieldValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO: Display each variant in a human-readable format
        // e.g., Int(42) -> "42", Null -> "NULL", Str("hello") -> "hello"
        // todo!("implement Display for FieldValue")
        match self {
            FieldValue::Int(i) => {
                write!(f, "{}", i)
            }
            FieldValue::Float(fmt) => {
                write!(f, "{}", fmt)
            }
            FieldValue::Str(s) => {
                write!(f, "{}", s)
            }
            FieldValue::Bool(b) => {
                if *b {
                    write!(f, "true")
                } else {
                    write!(f, "false")
                }
            }
            FieldValue::Null => {
                write!(f, "null")
            }
        }
    }
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO: Display the type name, e.g., Int -> "INT", Str -> "STRING"
        // todo!("implement Display for DataType")
        match self {
            DataType::Int => {
                write!(f, "int")
            }
            DataType::Float => {
                write!(f, "float")
            }
            DataType::Str => {
                write!(f, "str")
            }
            DataType::Bool => {
                write!(f, "bool")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── FieldValue Display ──────────────────────────────────

    #[test]
    fn display_value_int() {
        assert_eq!(FieldValue::Int(42).to_string(), "42");
        assert_eq!(FieldValue::Int(-1).to_string(), "-1");
        assert_eq!(FieldValue::Int(0).to_string(), "0");
    }

    #[test]
    fn display_value_float() {
        assert_eq!(FieldValue::Float(3.14).to_string(), "3.14");
        assert_eq!(FieldValue::Float(-0.5).to_string(), "-0.5");
    }

    #[test]
    fn display_value_str() {
        assert_eq!(FieldValue::Str("hello".into()).to_string(), "hello");
        assert_eq!(FieldValue::Str("".into()).to_string(), "");
    }

    #[test]
    fn display_value_bool() {
        assert_eq!(FieldValue::Bool(true).to_string(), "true");
        assert_eq!(FieldValue::Bool(false).to_string(), "false");
    }

    #[test]
    fn display_value_null() {
        assert_eq!(FieldValue::Null.to_string(), "null");
    }

    // ── DataType Display ───────────────────────────────

    #[test]
    fn display_datatype() {
        assert_eq!(DataType::Int.to_string(), "int");
        assert_eq!(DataType::Float.to_string(), "float");
        assert_eq!(DataType::Str.to_string(), "str");
        assert_eq!(DataType::Bool.to_string(), "bool");
    }

    // ── Clone / PartialEq ──────────────────────────────

    #[test]
    fn value_clone_and_eq() {
        let v = FieldValue::Str("test".into());
        assert_eq!(v.clone(), v);
    }

    #[test]
    fn value_ne() {
        assert_ne!(FieldValue::Int(1), FieldValue::Int(2));
        assert_ne!(FieldValue::Int(1), FieldValue::Null);
    }

    #[test]
    fn datatype_clone_and_eq() {
        assert_eq!(DataType::Int.clone(), DataType::Int);
        assert_ne!(DataType::Int, DataType::Float);
    }

    // ── Debug ──────────────────────────────────────────

    #[test]
    fn value_debug() {
        // Ensure Debug derive works and contains variant name
        let dbg = format!("{:?}", FieldValue::Int(7));
        assert!(dbg.contains("Int"));
    }
}
