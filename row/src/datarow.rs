use expr::schema::Schema;
use expr::types::FieldValue;

use crate::codec::RowCodec;
use crate::types::{RowKey, RowValue};

/// A complete data row holding both the encoded key and value.
#[derive(Debug, Clone)]
pub struct DataRow {
    pub key: RowKey,
    pub value: RowValue,
}

impl DataRow {
    pub fn new(key: RowKey, value: RowValue) -> Self {
        Self { key, value }
    }

    /// Decode all columns (PK + non-PK) back into Values.
    pub fn decode(&self, codec: &RowCodec, schema: &Schema) -> Vec<FieldValue> {
        codec.decode(&self.key, &self.value, schema)
    }

    /// Read a single PK column without full decode.
    pub fn read_pk_column<'a>(&'a self, codec: &RowCodec, col_index: usize) -> &'a [u8] {
        codec.read_pk_column(&self.key, col_index)
    }

    /// Compare this row's key to another row's key.
    pub fn compare_key(&self, other: &DataRow, num_pk_cols: usize) -> std::cmp::Ordering {
        self.key.compare(&other.key, num_pk_cols)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use expr::schema::Field;
    use expr::types::DataType;

    #[test]
    fn datarow_roundtrip() {
        let schema = Schema::new(vec![
            Field::new("id", DataType::Int),
            Field::new("name", DataType::Str),
        ]);
        let codec = RowCodec::new(1);
        let values = vec![FieldValue::Int(42), FieldValue::Str("alice".into())];

        let (key, val) = codec.encode(&values);
        let row = DataRow::new(key, val);

        assert_eq!(row.decode(&codec, &schema), values);
    }

    #[test]
    fn datarow_compare() {
        let codec = RowCodec::new(1);
        let (k1, v1) = codec.encode(&[FieldValue::Int(1), FieldValue::Str("a".into())]);
        let (k2, v2) = codec.encode(&[FieldValue::Int(2), FieldValue::Str("b".into())]);

        let r1 = DataRow::new(k1, v1);
        let r2 = DataRow::new(k2, v2);

        assert_eq!(r1.compare_key(&r2, 1), std::cmp::Ordering::Less);
    }
}
