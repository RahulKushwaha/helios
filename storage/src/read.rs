//! Storage read trait and RocksDB implementation.
//!
//! Provides low-level read primitives over the primary key index and
//! secondary indexes. All methods operate on raw [`DataRow`] or [`RowKey`]
//! values without decoding column data, keeping the storage layer
//! independent of schema interpretation.
//!
//! The read layer is intentionally thin:
//! - `scan` and `point_lookup`/`batch_get` access the primary key index.
//! - `index_scan` accesses a secondary index and returns primary keys.
//! - Materialization (fetching full rows from index results) is handled
//!   by the [`Materializer`](crate::materializer::Materializer) layer.

use row::DataRow;
use row::types::{RowKey, RowValue};

use crate::table::Table;
use crate::RocksStorage;

/// Low-level read operations over a storage backend.
///
/// All range operations use half-open intervals: `[start, end)`.
/// Pass `None` for either bound to leave it unbounded.
pub trait StorageRead {
    /// Scan the primary key index within `[start, end)`.
    ///
    /// Returns rows in primary key order. With both bounds as `None`,
    /// this is a full table scan.
    fn scan(&self, table: &Table, start: Option<&RowKey>, end: Option<&RowKey>) -> Vec<DataRow>;

    /// Point lookup: fetch a single row by its primary key.
    fn point_lookup(&self, table: &Table, key: &RowKey) -> Option<DataRow>;

    /// Batch point lookup: fetch multiple rows by primary key in a single
    /// round-trip. Uses RocksDB's `multi_get_cf` internally. Rows whose
    /// keys are not found are silently omitted from the result.
    fn batch_get(&self, table: &Table, keys: &[RowKey]) -> Vec<DataRow>;

    /// Secondary index scan within `[start, end)`.
    ///
    /// `column` identifies which secondary index to scan. The bounds are
    /// encoded index value prefixes (produced by
    /// [`RocksStorage::encode_index_prefix`]). Returns the primary keys
    /// of matching rows, which can then be materialized via
    /// [`batch_get`](StorageRead::batch_get) or the
    /// [`Materializer`](crate::materializer::Materializer).
    fn index_scan(
        &self,
        table: &Table,
        column: &str,
        start: Option<&RowKey>,
        end: Option<&RowKey>,
    ) -> Vec<RowKey>;
}

impl StorageRead for RocksStorage {
    fn scan(&self, table: &Table, start: Option<&RowKey>, end: Option<&RowKey>) -> Vec<DataRow> {
        let name = table.name();
        let cf = match self.db.cf_handle(&name) {
            Some(cf) => cf,
            None => return Vec::new(),
        };

        let mode = match start {
            Some(k) => rocksdb::IteratorMode::From(&k.0, rocksdb::Direction::Forward),
            None => rocksdb::IteratorMode::Start,
        };

        let iter = self.db.iterator_cf(cf, mode);
        let mut rows = Vec::new();
        for item in iter {
            let (key, val) = item.expect("scan iteration failed");
            if let Some(end_key) = end {
                if key.as_ref() >= end_key.0.as_slice() {
                    break;
                }
            }
            rows.push(DataRow::new(RowKey(key.to_vec()), RowValue(val.to_vec())));
        }
        rows
    }

    fn point_lookup(&self, table: &Table, key: &RowKey) -> Option<DataRow> {
        let cf = self.db.cf_handle(&table.name())?;
        let val = self.db.get_cf(cf, &key.0).ok()??;
        Some(DataRow::new(key.clone(), RowValue(val.to_vec())))
    }

    fn batch_get(&self, table: &Table, keys: &[RowKey]) -> Vec<DataRow> {
        let name = table.name();
        let cf = match self.db.cf_handle(&name) {
            Some(cf) => cf,
            None => return Vec::new(),
        };
        let cf_keys: Vec<_> = keys.iter().map(|k| (&cf, k.0.as_slice())).collect();
        self.db
            .multi_get_cf(cf_keys)
            .into_iter()
            .zip(keys)
            .filter_map(|(result, key)| {
                result.ok()?.map(|val| DataRow::new(key.clone(), RowValue(val.to_vec())))
            })
            .collect()
    }

    fn index_scan(
        &self,
        table: &Table,
        column: &str,
        start: Option<&RowKey>,
        end: Option<&RowKey>,
    ) -> Vec<RowKey> {
        let name = table.name();
        let cf_name = Self::index_cf_name(&name, column);
        let idx_cf = match self.db.cf_handle(&cf_name) {
            Some(cf) => cf,
            None => return Vec::new(),
        };

        let mode = match start {
            Some(k) => rocksdb::IteratorMode::From(&k.0, rocksdb::Direction::Forward),
            None => rocksdb::IteratorMode::Start,
        };

        let mut keys = Vec::new();
        let iter = self.db.iterator_cf(idx_cf, mode);

        // Index key layout: [value_prefix | pk_bytes | pk_len(u16)]
        // The last 2 bytes encode the length of the embedded primary key,
        // allowing us to split the value prefix from the PK on read.
        for item in iter {
            let (key_bytes, _) = item.unwrap();
            let raw = key_bytes.as_ref();
            let total = raw.len();
            let pk_len = u16::from_be_bytes([raw[total - 2], raw[total - 1]]) as usize;
            let value_part = &raw[..total - 2 - pk_len];

            if let Some(end_key) = end {
                if value_part >= end_key.0.as_slice() {
                    break;
                }
            }

            let pk_bytes = &raw[total - 2 - pk_len..total - 2];
            keys.push(RowKey(pk_bytes.to_vec()));
        }
        keys
    }
}
