use row::DataRow;
use row::types::RowKey;

use crate::read::StorageRead;
use crate::table::Table;

/// Materializes rows from primary keys returned by an index scan.
pub struct Materializer<'a, S: StorageRead> {
    storage: &'a S,
}

impl<'a, S: StorageRead> Materializer<'a, S> {
    pub fn new(storage: &'a S) -> Self {
        Self { storage }
    }

    /// Fetch full rows for a set of primary keys.
    pub fn materialize(&self, table: &Table, keys: &[RowKey]) -> Vec<DataRow> {
        self.storage.batch_get(table, keys)
    }
}
