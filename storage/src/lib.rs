pub mod catalog;
pub mod engine;
pub mod materializer;
pub mod read;
pub mod table;
pub mod write;

pub use catalog::Catalog;
pub use materializer::Materializer;
pub use read::StorageRead;
pub use write::StorageWrite;

use expr::schema::{Field, Schema};
use expr::types::{DataType, FieldValue};
use rocksdb::{ColumnFamilyDescriptor, Options, DB};
use row::RowCodec;
use row::types::RowKey;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

const META_CF: &str = "_meta";

/// Meta row schema: PK = table_name (Str), value = next_row_id (Int), schema_str (Str), indexes_str (Str)
fn meta_schema() -> Schema {
    Schema::new(vec![
        Field::new("table_name", DataType::Str),
        Field::new("next_row_id", DataType::Int),
        Field::new("schema_str", DataType::Str),
        Field::new("indexes_str", DataType::Str),
    ])
}

fn meta_codec() -> RowCodec {
    RowCodec::new(1)
}

pub(crate) struct TableMeta {
    pub(crate) schema: Vec<(String, DataType, bool)>, // (name, type, is_pk)
    pub(crate) next_row_id: u64,
    pub(crate) indexes: Vec<String>,
}

impl TableMeta {
    fn encode_schema_str(&self) -> String {
        self.schema
            .iter()
            .map(|(name, dt, pk)| format!("{}:{}:{}", name, dt, if *pk { "1" } else { "0" }))
            .collect::<Vec<_>>()
            .join(",")
    }

    fn encode_indexes_str(&self) -> String {
        self.indexes.join(",")
    }

    fn decode_schema_str(s: &str) -> Vec<(String, DataType, bool)> {
        if s.is_empty() {
            return Vec::new();
        }
        s.split(',')
            .map(|pair| {
                let parts: Vec<&str> = pair.splitn(3, ':').collect();
                let name = parts[0].to_string();
                let dt = match parts[1] {
                    "int" => DataType::Int,
                    "float" => DataType::Float,
                    "str" => DataType::Str,
                    "bool" => DataType::Bool,
                    other => panic!("unknown data type: {}", other),
                };
                let is_pk = parts.get(2).map(|v| *v == "1").unwrap_or(false);
                (name, dt, is_pk)
            })
            .collect()
    }

    fn decode_indexes_str(s: &str) -> Vec<String> {
        if s.is_empty() {
            return Vec::new();
        }
        s.split(',').map(|s| s.to_string()).collect()
    }
}

/// RocksDB-backed storage layer for the query engine.
pub struct RocksStorage {
    pub(crate) db: DB,
    pub(crate) meta_cache: Mutex<HashMap<String, TableMeta>>,
}

impl RocksStorage {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let cf_names = DB::list_cf(&Options::default(), path).unwrap_or_default();

        let mut cf_descriptors = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for name in [String::from("default"), String::from(META_CF)]
            .into_iter()
            .chain(cf_names.into_iter())
        {
            if seen.insert(name.clone()) {
                cf_descriptors.push(ColumnFamilyDescriptor::new(&name, Options::default()));
            }
        }

        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);

        let db = DB::open_cf_descriptors(&db_opts, path, cf_descriptors)
            .expect("failed to open RocksDB");

        let storage = Self {
            db,
            meta_cache: Mutex::new(HashMap::new()),
        };
        storage.load_meta_cache();
        storage
    }

    pub(crate) fn load_meta_cache(&self) {
        let cf = self.db.cf_handle(META_CF).unwrap();
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);
        let mut cache = self.meta_cache.lock().unwrap();
        let codec = meta_codec();
        let schema = meta_schema();
        for item in iter {
            let (key_bytes, val_bytes) = item.expect("meta iteration failed");
            let rk = row::RowKey(key_bytes.to_vec());
            let rv = row::RowValue(val_bytes.to_vec());
            let values = codec.decode(&rk, &rv, &schema);

            let table_name = match &values[0] {
                FieldValue::Str(s) => s.clone(),
                _ => continue,
            };
            let next_row_id = match &values[1] {
                FieldValue::Int(i) => *i as u64,
                _ => 0,
            };
            let schema_str = match &values[2] {
                FieldValue::Str(s) => s.as_str(),
                _ => "",
            };
            let indexes_str = match &values[3] {
                FieldValue::Str(s) => s.as_str(),
                _ => "",
            };

            cache.insert(table_name, TableMeta {
                schema: TableMeta::decode_schema_str(schema_str),
                next_row_id,
                indexes: TableMeta::decode_indexes_str(indexes_str),
            });
        }
    }

    pub(crate) fn save_meta(&self, table_name: &str, meta: &TableMeta) {
        let cf = self.db.cf_handle(META_CF).unwrap();
        let codec = meta_codec();
        let values = vec![
            FieldValue::Str(table_name.to_string()),
            FieldValue::Int(meta.next_row_id as i64),
            FieldValue::Str(meta.encode_schema_str()),
            FieldValue::Str(meta.encode_indexes_str()),
        ];
        let (rk, rv) = codec.encode(&values);
        self.db.put_cf(cf, &rk.0, &rv.0).unwrap();
    }

    /// Encode a data row into bytes using the row format.
    pub(crate) fn encode_data_row(values: &[FieldValue]) -> Vec<u8> {
        let mut buf = Vec::new();
        for v in values {
            row::encoding::encode_value(v, &mut buf);
        }
        buf
    }

    /// Decode a data row from bytes using the row format (no PK split).
    pub(crate) fn decode_data_row(bytes: &[u8], schema: &Schema) -> Vec<FieldValue> {
        let mut result = Vec::with_capacity(schema.fields.len());
        let mut pos = 0;
        for field in &schema.fields {
            let (v, consumed) = row::encoding::decode_value_with_len(&bytes[pos..], &field.data_type);
            result.push(v);
            pos += consumed;
        }
        result
    }

    /// Decode a DataRow using the table's PK info.
    /// If the table has PK columns, uses RowCodec to decode key + value.
    /// Otherwise, decodes all columns from the value bytes.
    pub fn decode_datarow(dr: &row::DataRow, table: &crate::table::Table) -> Vec<FieldValue> {
        let schema = table.schema();
        let pk_cols = table.pk_columns();
        if !pk_cols.is_empty() {
            let pk_positions: Vec<usize> = pk_cols.iter().map(|c| c.col_pos).collect();
            let codec = row::RowCodec::from_pk_positions(pk_positions);
            codec.decode(&dr.key, &dr.value, &schema)
        } else {
            Self::decode_data_row(&dr.value.0, &schema)
        }
    }

    pub(crate) fn index_cf_name(table: &str, column: &str) -> String {
        format!("idx_{}_{}", table, column)
    }

    /// Encode just the value portion of an index key (no pk suffix).
    pub fn encode_index_prefix(value: &FieldValue) -> RowKey {
        let full = Self::encode_index_key(value, &[]);
        RowKey(full[..full.len() - 2].to_vec())
    }

    pub(crate) fn encode_index_key(value: &FieldValue, pk_bytes: &[u8]) -> Vec<u8> {
        let mut key = Vec::new();
        match value {
            FieldValue::Null => key.push(0),
            FieldValue::Bool(b) => {
                key.push(1);
                key.push(if *b { 1 } else { 0 });
            }
            FieldValue::Int(i) => {
                key.push(2);
                key.extend_from_slice(&((*i as u64) ^ (1u64 << 63)).to_be_bytes());
            }
            FieldValue::Float(f) => {
                key.push(3);
                let bits = f.to_bits();
                let ordered = if *f >= 0.0 {
                    bits ^ (1u64 << 63)
                } else {
                    !bits
                };
                key.extend_from_slice(&ordered.to_be_bytes());
            }
            FieldValue::Str(s) => {
                key.push(4);
                key.extend_from_slice(s.as_bytes());
                key.push(0);
            }
        }
        // Append pk_bytes then pk_len (u16) so we can split on read.
        key.extend_from_slice(pk_bytes);
        key.extend_from_slice(&(pk_bytes.len() as u16).to_be_bytes());
        key
    }

    pub(crate) fn write_index_entry(&self, table: &str, column: &str, value: &FieldValue, pk_bytes: &[u8]) {
        let cf_name = Self::index_cf_name(table, column);
        if let Some(cf) = self.db.cf_handle(&cf_name) {
            let key = Self::encode_index_key(value, pk_bytes);
            self.db.put_cf(cf, key, &[]).unwrap();
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::materializer::Materializer;
    use expr::schema::{Field, Schema};
    use expr::types::{DataType, FieldValue};
    use tempfile::tempdir;

    fn test_schema() -> Schema {
        Schema::new(vec![
            Field::new("id", DataType::Int),
            Field::new("name", DataType::Str),
            Field::new("score", DataType::Int),
        ])
    }

    fn decode_rows(rows: &[row::DataRow], table: &crate::table::Table) -> Vec<Vec<FieldValue>> {
        rows.iter().map(|dr| RocksStorage::decode_datarow(dr, table)).collect()
    }

    #[test]
    fn roundtrip_create_insert_scan() {
        let dir = tempdir().unwrap();
        let mut storage = RocksStorage::new(dir.path());
        storage.create_table("users", &test_schema());
        let t = storage.get_table("users").unwrap();

        storage.insert_row(&t, vec![FieldValue::Int(1), FieldValue::Str("alice".into()), FieldValue::Int(90)]);
        storage.insert_row(&t, vec![FieldValue::Int(2), FieldValue::Str("bob".into()), FieldValue::Int(75)]);

        let data_rows = storage.scan(&t, None, None);
        let rows = decode_rows(&data_rows, &t);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][1], FieldValue::Str("alice".into()));
        assert_eq!(rows[1][2], FieldValue::Int(75));
    }

    #[test]
    fn point_lookup_found() {
        let dir = tempdir().unwrap();
        let mut storage = RocksStorage::new(dir.path());
        storage.create_table("t", &test_schema());
        let t = storage.get_table("t").unwrap();
        storage.insert_row(&t, vec![FieldValue::Int(1), FieldValue::Str("a".into()), FieldValue::Int(10)]);

        let all = storage.scan(&t, None, None);
        let key = &all[0].key;
        let found = storage.point_lookup(&t, key).unwrap();
        let row = RocksStorage::decode_datarow(&found, &t);
        assert_eq!(row[1], FieldValue::Str("a".into()));
    }

    #[test]
    fn get_schema_roundtrip() {
        let dir = tempdir().unwrap();
        let mut storage = RocksStorage::new(dir.path());
        storage.create_table("t", &test_schema());

        let got = Catalog::get_schema(&storage, "t").unwrap();
        assert_eq!(got.fields.len(), 3);
        assert_eq!(got.fields[0].name, "id");
        assert_eq!(got.fields[2].data_type, DataType::Int);
    }

    #[test]
    fn list_tables() {
        let dir = tempdir().unwrap();
        let mut storage = RocksStorage::new(dir.path());
        storage.create_table("a", &Schema::new(vec![Field::new("x", DataType::Int)]));
        storage.create_table("b", &Schema::new(vec![Field::new("y", DataType::Str)]));

        let mut tables = Catalog::list_tables(&storage);
        tables.sort();
        assert_eq!(tables, vec!["a", "b"]);
    }

    /// Helper: compute [start, end) for Eq on an index value.
    fn eq_bounds(val: &FieldValue) -> (row::types::RowKey, Option<row::types::RowKey>) {
        let prefix = RocksStorage::encode_index_prefix(val);
        let mut next = prefix.0.clone();
        let has_next = {
            let mut ok = false;
            for i in (0..next.len()).rev() {
                if next[i] < 0xFF { next[i] += 1; ok = true; break; }
                next[i] = 0;
            }
            ok
        };
        (prefix, if has_next { Some(row::types::RowKey(next)) } else { None })
    }

    #[test]
    fn index_scan_returns_keys() {
        let dir = tempdir().unwrap();
        let mut storage = RocksStorage::new(dir.path());
        storage.create_table("t", &test_schema());
        let t = storage.get_table("t").unwrap();

        storage.insert_row(&t, vec![FieldValue::Int(1), FieldValue::Str("a".into()), FieldValue::Int(10)]);
        storage.insert_row(&t, vec![FieldValue::Int(2), FieldValue::Str("b".into()), FieldValue::Int(20)]);
        storage.insert_row(&t, vec![FieldValue::Int(3), FieldValue::Str("c".into()), FieldValue::Int(10)]);

        storage.create_index("t", "score");
        let t = storage.get_table("t").unwrap();

        let (start, end) = eq_bounds(&FieldValue::Int(10));
        let keys = storage.index_scan(&t, "score", Some(&start), end.as_ref());
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn index_scan_materialize() {
        let dir = tempdir().unwrap();
        let mut storage = RocksStorage::new(dir.path());
        storage.create_table("t", &test_schema());
        let t = storage.get_table("t").unwrap();

        storage.insert_row(&t, vec![FieldValue::Int(1), FieldValue::Str("a".into()), FieldValue::Int(10)]);
        storage.insert_row(&t, vec![FieldValue::Int(2), FieldValue::Str("b".into()), FieldValue::Int(20)]);
        storage.insert_row(&t, vec![FieldValue::Int(3), FieldValue::Str("c".into()), FieldValue::Int(10)]);

        storage.create_index("t", "score");
        let t = storage.get_table("t").unwrap();

        let (start, end) = eq_bounds(&FieldValue::Int(10));
        let keys = storage.index_scan(&t, "score", Some(&start), end.as_ref());
        let mat = Materializer::new(&storage);
        let data_rows = mat.materialize(&t, &keys);
        let rows = decode_rows(&data_rows, &t);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r[2] == FieldValue::Int(10)));
    }

    #[test]
    fn index_scan_gt() {
        let dir = tempdir().unwrap();
        let mut storage = RocksStorage::new(dir.path());
        storage.create_table("t", &test_schema());
        let t = storage.get_table("t").unwrap();

        for i in 0..5 {
            storage.insert_row(&t, vec![FieldValue::Int(i), FieldValue::Str(format!("r{}", i)), FieldValue::Int(i * 10)]);
        }
        storage.create_index("t", "score");
        let t = storage.get_table("t").unwrap();

        // Gt(20): start at next_prefix(20), no end
        let (_, next) = eq_bounds(&FieldValue::Int(20));
        let keys = storage.index_scan(&t, "score", next.as_ref(), None);
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn insert_after_index_creation() {
        let dir = tempdir().unwrap();
        let mut storage = RocksStorage::new(dir.path());
        storage.create_table("t", &test_schema());
        storage.create_index("t", "score");
        let t = storage.get_table("t").unwrap();

        storage.insert_row(&t, vec![FieldValue::Int(1), FieldValue::Str("a".into()), FieldValue::Int(50)]);
        storage.insert_row(&t, vec![FieldValue::Int(2), FieldValue::Str("b".into()), FieldValue::Int(60)]);

        let (start, end) = eq_bounds(&FieldValue::Int(50));
        let keys = storage.index_scan(&t, "score", Some(&start), end.as_ref());
        let mat = Materializer::new(&storage);
        let data_rows = mat.materialize(&t, &keys);
        let rows = decode_rows(&data_rows, &t);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][1], FieldValue::Str("a".into()));
    }

    #[test]
    fn meta_persists_across_reopen() {
        let dir = tempdir().unwrap();
        {
            let mut storage = RocksStorage::new(dir.path());
            storage.create_table("t", &test_schema());
            let t = storage.get_table("t").unwrap();
            storage.insert_row(&t, vec![FieldValue::Int(1), FieldValue::Str("a".into()), FieldValue::Int(10)]);
            storage.create_index("t", "score");
        }
        let storage = RocksStorage::new(dir.path());
        let schema = Catalog::get_schema(&storage, "t").unwrap();
        assert_eq!(schema.fields.len(), 3);
        assert!(Catalog::has_index(&storage, "t", "score"));
        assert_eq!(Catalog::list_tables(&storage), vec!["t"]);
    }

    #[test]
    fn insert_scan_with_primary_key() {
        let dir = tempdir().unwrap();
        let mut storage = RocksStorage::new(dir.path());
        let schema = Schema::new(vec![
            Field::new("id", DataType::Int).with_pk(true).with_pos(0),
            Field::new("name", DataType::Str).with_pos(1),
            Field::new("score", DataType::Int).with_pos(2),
        ]);
        storage.create_table("pk_table", &schema);
        let t = storage.get_table("pk_table").unwrap();

        storage.insert_row(&t, vec![FieldValue::Int(1), FieldValue::Str("alice".into()), FieldValue::Int(90)]);
        storage.insert_row(&t, vec![FieldValue::Int(2), FieldValue::Str("bob".into()), FieldValue::Int(75)]);

        let data_rows = storage.scan(&t, None, None);
        let rows = decode_rows(&data_rows, &t);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec![FieldValue::Int(1), FieldValue::Str("alice".into()), FieldValue::Int(90)]);
        assert_eq!(rows[1], vec![FieldValue::Int(2), FieldValue::Str("bob".into()), FieldValue::Int(75)]);
    }

    #[test]
    #[test]
    fn scan_with_range_bounds() {
        let dir = tempdir().unwrap();
        let mut storage = RocksStorage::new(dir.path());
        let schema = Schema::new(vec![
            Field::new("id", DataType::Int).with_pk(true).with_pos(0),
            Field::new("name", DataType::Str).with_pos(1),
        ]);
        storage.create_table("t", &schema);
        let t = storage.get_table("t").unwrap();

        for i in 1..=5 {
            storage.insert_row(&t, vec![FieldValue::Int(i), FieldValue::Str(format!("r{}", i))]);
        }

        // Full scan returns all 5.
        assert_eq!(storage.scan(&t, None, None).len(), 5);

        // Get keys for id=2 and id=4 to use as bounds.
        let all = storage.scan(&t, None, None);
        let key2 = &all[1].key; // id=2
        let key4 = &all[3].key; // id=4

        // Range [2, 4) should return 2 rows: id=2, id=3.
        let range = storage.scan(&t, Some(key2), Some(key4));
        let rows = decode_rows(&range, &t);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0], FieldValue::Int(2));
        assert_eq!(rows[1][0], FieldValue::Int(3));

        // Start bound only: [3, ...)
        let from3 = storage.scan(&t, Some(&all[2].key), None);
        assert_eq!(from3.len(), 3);

        // End bound only: (..., 2) returns 1 row: id=1
        let to2 = storage.scan(&t, None, Some(key2));
        assert_eq!(to2.len(), 1);
    }

    fn insert_scan_pk_not_first_column() {
        let dir = tempdir().unwrap();
        let mut storage = RocksStorage::new(dir.path());
        let schema = Schema::new(vec![
            Field::new("name", DataType::Str).with_pos(0),
            Field::new("age", DataType::Int).with_pos(1),
            Field::new("email", DataType::Str).with_pk(true).with_pos(2),
        ]);
        storage.create_table("t", &schema);
        let t = storage.get_table("t").unwrap();

        storage.insert_row(&t, vec![FieldValue::Str("alice".into()), FieldValue::Int(30), FieldValue::Str("a@b.com".into())]);
        storage.insert_row(&t, vec![FieldValue::Str("bob".into()), FieldValue::Int(25), FieldValue::Str("b@b.com".into())]);

        let data_rows = storage.scan(&t, None, None);
        let rows = decode_rows(&data_rows, &t);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0], FieldValue::Str("alice".into()));
        assert_eq!(rows[0][1], FieldValue::Int(30));
        assert_eq!(rows[0][2], FieldValue::Str("a@b.com".into()));
    }
}
