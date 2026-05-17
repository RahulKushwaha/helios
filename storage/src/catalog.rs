//! Catalog trait and RocksDB implementation.
//!
//! The catalog is responsible for DDL operations: creating tables, managing
//! schemas, and maintaining secondary index metadata. It persists table
//! metadata (schema, indexes, next row ID) in a dedicated `__meta` column
//! family using the row format encoding.

use expr::schema::{Field, Schema};
use expr::types::DataType;
use rocksdb::Options;

use crate::table::Table;
use crate::{RocksStorage, TableMeta, META_CF};

/// Catalog manages table and index metadata.
///
/// This trait is intentionally separate from [`StorageRead`] and [`StorageWrite`]
/// so that read/write operations can depend on a `Table` object obtained from
/// the catalog without circular coupling.
pub trait Catalog {
    /// Create a new table with the given schema.
    ///
    /// Creates a RocksDB column family for the table's data and persists
    /// the schema and initial metadata to the `__meta` CF.
    fn create_table(&mut self, name: &str, schema: &Schema);

    /// Retrieve a [`Table`] object by name.
    ///
    /// The returned `Table` contains the full schema (with PK flags),
    /// index metadata, and row ID state. Returns `None` if the table
    /// does not exist.
    fn get_table(&self, name: &str) -> Option<Table>;

    /// Retrieve just the schema for a table.
    fn get_schema(&self, table: &str) -> Option<Schema>;

    /// List all table names in the catalog.
    fn list_tables(&self) -> Vec<String>;

    /// Create a secondary index on a column.
    ///
    /// This creates a new column family for the index and backfills it
    /// by scanning all existing rows. Future inserts will maintain the
    /// index automatically via [`StorageWrite::insert_row`].
    fn create_index(&mut self, table: &str, column: &str);

    /// Check whether a secondary index exists for the given table and column.
    fn has_index(&self, table: &str, column: &str) -> bool;
}

impl Catalog for RocksStorage {
    fn create_table(&mut self, name: &str, schema: &Schema) {
        self.db
            .create_cf(name, &Options::default())
            .unwrap_or_else(|_| {});

        let meta = TableMeta {
            schema: schema
                .fields
                .iter()
                .map(|f| (f.name.clone(), f.data_type.clone(), f.is_pk))
                .collect(),
            next_row_id: 0,
            indexes: Vec::new(),
        };
        self.save_meta(name, &meta);
        self.meta_cache
            .lock()
            .unwrap()
            .insert(name.to_string(), meta);
    }

    fn get_schema(&self, table: &str) -> Option<Schema> {
        let cache = self.meta_cache.lock().unwrap();
        let meta = cache.get(table)?;
        let fields: Vec<Field> = meta
            .schema
            .iter()
            .enumerate()
            .map(|(pos, (name, dt, is_pk))| {
                Field::new(name.clone(), dt.clone()).with_pk(*is_pk).with_pos(pos)
            })
            .collect();
        Some(Schema::new(fields))
    }

    fn get_table(&self, name: &str) -> Option<Table> {
        let cache = self.meta_cache.lock().unwrap();
        let meta = cache.get(name)?;
        let cols: Vec<Field> = meta
            .schema
            .iter()
            .enumerate()
            .map(|(pos, (col_name, dt, is_pk))| {
                Field::new(col_name.clone(), dt.clone()).with_pk(*is_pk).with_pos(pos)
            })
            .collect();
        let table = Table::new(0, name, cols);
        table.set_next_row_id(meta.next_row_id);
        for idx_col in &meta.indexes {
            table.add_index(crate::table::Index {
                name: format!("idx_{}_{}", name, idx_col),
                table_id: 0,
                columns: vec![idx_col.clone()],
            });
        }
        Some(table)
    }

    fn list_tables(&self) -> Vec<String> {
        self.meta_cache.lock().unwrap().keys().cloned().collect()
    }

    fn create_index(&mut self, table: &str, column: &str) {
        let cf_name = Self::index_cf_name(table, column);
        self.db
            .create_cf(&cf_name, &Options::default())
            .unwrap_or(());

        // Backfill: scan all existing rows and write index entries.
        let t = Catalog::get_table(self, table).expect("table not found");
        let schema = t.schema();
        let col_idx = schema
            .field_by_name(column)
            .expect("column not found in schema")
            .0;

        let table_cf = self.db.cf_handle(table).unwrap();
        let iter = self.db.iterator_cf(table_cf, rocksdb::IteratorMode::Start);
        for item in iter {
            let (key_bytes, val) = item.unwrap();
            let dr = row::DataRow::new(
                row::types::RowKey(key_bytes.to_vec()),
                row::types::RowValue(val.to_vec()),
            );
            let row = Self::decode_datarow(&dr, &t);
            self.write_index_entry(table, column, &row[col_idx], &key_bytes);
        }

        // Persist the index in metadata.
        let mut cache = self.meta_cache.lock().unwrap();
        let meta = cache.get_mut(table).unwrap();
        if !meta.indexes.contains(&column.to_string()) {
            meta.indexes.push(column.to_string());
            self.save_meta(table, meta);
        }
    }

    fn has_index(&self, table: &str, column: &str) -> bool {
        let cache = self.meta_cache.lock().unwrap();
        cache
            .get(table)
            .map(|m| m.indexes.contains(&column.to_string()))
            .unwrap_or(false)
    }
}
