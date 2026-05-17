use crate::catalog::Catalog;
use crate::materializer::Materializer;
use crate::read::StorageRead;
use crate::RocksStorage;
use execution::engine::{DataSource, ExecutionEngine, ExecutionError, ResultSet, Row};
use execution::stream::build_stream;
use expr::expr::{Expr, Operator};
use expr::schema::Schema;
use expr::types::FieldValue;
use physical_plan::plan::PhysicalPlan;
use row::types::RowKey;

/// Execution engine backed by any storage implementing Catalog + StorageRead.
pub struct RocksEngine<'a, S: Catalog + StorageRead> {
    pub storage: &'a S,
}

impl<'a, S: Catalog + StorageRead> RocksEngine<'a, S> {
    pub fn new(storage: &'a S) -> Self {
        Self { storage }
    }
}

/// Adapter that implements DataSource by scanning from storage.
struct StorageDataSource<'a, S: Catalog + StorageRead> {
    storage: &'a S,
}

impl<'a, S: Catalog + StorageRead> DataSource for StorageDataSource<'a, S> {
    fn scan(&self, table_name: &str, _schema: &Schema) -> Result<ResultSet, ExecutionError> {
        let table = self
            .storage
            .get_table(table_name)
            .ok_or_else(|| ExecutionError::TableNotFound(table_name.into()))?;
        let data_rows = self.storage.scan(&table, None, None);
        Ok(data_rows
            .iter()
            .map(|dr| RocksStorage::decode_datarow(dr, &table))
            .collect())
    }
}

impl<'a, S: Catalog + StorageRead> ExecutionEngine for RocksEngine<'a, S> {
    fn execute(&self, plan: &PhysicalPlan) -> Result<ResultSet, ExecutionError> {
        if let PhysicalPlan::Filter { predicate, input, .. } = plan {
            if let Some(rows) = self.try_index_scan(input, predicate) {
                return Ok(rows);
            }
        }

        let source = StorageDataSource { storage: self.storage };
        let mut stream = build_stream(plan, &source)?;
        let mut out = Vec::new();
        while let Some(batch) = stream.next_batch()? {
            out.extend(batch);
        }
        Ok(out)
    }
}

impl<'a, S: Catalog + StorageRead> RocksEngine<'a, S> {
    fn try_index_scan(&self, input: &PhysicalPlan, predicate: &Expr) -> Option<Vec<Row>> {
        let table_name = match input {
            PhysicalPlan::TableScan { table_name, .. } => table_name,
            _ => return None,
        };

        let (col, op, val) = match predicate {
            Expr::BinaryExpr { left, op, right } => match (left.as_ref(), right.as_ref()) {
                (Expr::Column(c), Expr::Literal(v)) => (c.as_str(), op, v),
                _ => return None,
            },
            _ => return None,
        };

        let table = self.storage.get_table(table_name)?;
        if !table.has_index(col) {
            return None;
        }

        let prefix = RocksStorage::encode_index_prefix(val);

        // Compute the exclusive upper bound by incrementing the prefix.
        let next_prefix = increment_bytes(&prefix.0).map(RowKey);

        let (start, end): (Option<&RowKey>, Option<&RowKey>) = match op {
            Operator::Eq => (Some(&prefix), next_prefix.as_ref()),
            Operator::Lt => (None, Some(&prefix)),
            Operator::LtEq => (None, next_prefix.as_ref()),
            Operator::Gt => (next_prefix.as_ref(), None),
            Operator::GtEq => (Some(&prefix), None),
            _ => return None,
        };

        let pk_keys = self.storage.index_scan(&table, col, start, end);
        let mat = Materializer::new(self.storage);
        let data_rows = mat.materialize(&table, &pk_keys);

        Some(
            data_rows
                .iter()
                .map(|dr| RocksStorage::decode_datarow(dr, &table))
                .collect(),
        )
    }
}

/// Increment a byte slice to produce the exclusive upper bound.
/// Returns None if all bytes are 0xFF (no upper bound).
fn increment_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut result = bytes.to_vec();
    for i in (0..result.len()).rev() {
        if result[i] < 0xFF {
            result[i] += 1;
            return Some(result);
        }
        result[i] = 0;
    }
    None
}
