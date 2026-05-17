//! Storage write trait and RocksDB implementation.
//!
//! Handles row insertion with automatic primary key encoding and
//! secondary index maintenance.

use expr::types::FieldValue;
use row::RowCodec;

use crate::table::Table;
use crate::RocksStorage;

/// Write operations over a storage backend.
pub trait StorageWrite {
    /// Insert a row into the table.
    ///
    /// Column values must be in schema order. The storage layer handles
    /// key/value encoding based on the table's primary key definition:
    ///
    /// - **With PK columns**: PK columns (identified by `is_pk` in the
    ///   schema) are encoded into the RocksDB key via [`RowCodec`], and
    ///   the remaining columns form the value. This means rows are
    ///   physically ordered by primary key in storage.
    ///
    /// - **Without PK columns**: An auto-increment row ID is used as the
    ///   RocksDB key, and all columns are encoded into the value.
    ///
    /// Any existing secondary indexes on the table are updated
    /// automatically.
    fn insert_row(&self, table: &Table, row: Vec<FieldValue>);
}

impl StorageWrite for RocksStorage {
    fn insert_row(&self, table: &Table, row: Vec<FieldValue>) {
        let name = table.name();
        let pk_cols = table.pk_columns();
        let cf = self.db.cf_handle(&name).expect("table CF not found");

        let (rk_bytes, rv_bytes) = if !pk_cols.is_empty() {
            // Encode PK columns into the key, non-PK columns into the value.
            let pk_positions: Vec<usize> = pk_cols.iter().map(|c| c.col_pos).collect();
            let codec = RowCodec::from_pk_positions(pk_positions);
            let (rk, rv) = codec.encode(&row);
            (rk.0, rv.0)
        } else {
            // No PK: use auto-increment row ID as the key.
            let row_id = {
                let mut cache = self.meta_cache.lock().unwrap();
                let meta = cache.get_mut(&name).expect("table not found");
                let id = meta.next_row_id;
                meta.next_row_id += 1;
                self.save_meta(&name, meta);
                id
            };
            (row_id.to_be_bytes().to_vec(), Self::encode_data_row(&row))
        };

        self.db.put_cf(cf, &rk_bytes, &rv_bytes).unwrap();

        // Maintain secondary indexes: write an entry for each indexed column.
        let schema = table.schema();
        for idx in table.indexes() {
            for col_name in &idx.columns {
                if let Some((col_idx, _)) = schema.field_by_name(col_name) {
                    self.write_index_entry(&name, col_name, &row[col_idx], &rk_bytes);
                }
            }
        }
    }
}
