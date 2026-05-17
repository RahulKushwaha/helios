// [File 27] ExecutionEngine trait
//
// This is the extension point for plugging in a custom execution engine.
// The in-memory engine implements this trait; your future custom engine will too.

use expr::schema::Schema;
use expr::types::FieldValue;
use physical_plan::plan::PhysicalPlan;
use std::fmt;

/// Rows are represented as Vec<FieldValue>, result set is Vec of rows.
pub type Row = Vec<FieldValue>;
pub type ResultSet = Vec<Row>;

#[derive(Debug)]
pub enum ExecutionError {
    /// Table not found in the data store.
    TableNotFound(String),
    /// Type mismatch during expression evaluation.
    TypeError(String),
    /// Generic execution error.
    Internal(String),
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TableNotFound(t) => write!(f, "Table not found: {t}"),
            Self::TypeError(msg) => write!(f, "Type error: {msg}"),
            Self::Internal(msg) => write!(f, "Execution error: {msg}"),
        }
    }
}

/// Trait for executing a PhysicalPlan and producing results.
///
/// Implement this trait to create a custom execution engine.
/// The optimizer produces a PhysicalPlan; any engine that implements
/// this trait can execute it.
pub trait ExecutionEngine {
    fn execute(&self, plan: &PhysicalPlan) -> Result<ResultSet, ExecutionError>;
}

/// Boundary between the execution engine and the storage layer.
/// Implement this to plug in any storage backend (RocksDB, in-memory, etc.)
/// — the executor calls `scan` to read rows for a `TableScan` operator.
pub trait DataSource {
    fn scan(&self, table_name: &str, schema: &Schema) -> Result<ResultSet, ExecutionError>;
}
