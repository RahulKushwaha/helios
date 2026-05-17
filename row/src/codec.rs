use expr::schema::Schema;
use expr::types::FieldValue;

use crate::encoding::{decode_value, decode_value_with_len, encode_value};
use crate::types::{RowKey, RowValue};

/// Encodes/decodes rows given the positions of PK columns.
/// PK columns (by position in schema order) form the key;
/// the remaining columns go into the value.
pub struct RowCodec {
    /// Schema positions of PK columns, in order.
    pk_positions: Vec<usize>,
}

impl RowCodec {
    /// Create a codec where the first `n` columns are PK.
    pub fn new(n: usize) -> Self {
        assert!(n > 0, "must have at least one PK column");
        Self { pk_positions: (0..n).collect() }
    }

    /// Create a codec from explicit PK column positions.
    pub fn from_pk_positions(positions: Vec<usize>) -> Self {
        assert!(!positions.is_empty(), "must have at least one PK column");
        Self { pk_positions: positions }
    }

    pub fn num_pk_cols(&self) -> usize {
        self.pk_positions.len()
    }

    pub fn pk_positions(&self) -> &[usize] {
        &self.pk_positions
    }

    /// Encode a full row (all columns in schema order) into key + value.
    pub fn encode(&self, values: &[FieldValue]) -> (RowKey, RowValue) {
        let n = self.pk_positions.len();

        // Encode PK columns into key.
        let mut key_data = Vec::new();
        let mut end_offsets: Vec<u16> = Vec::new();
        for &pos in &self.pk_positions {
            encode_value(&values[pos], &mut key_data);
            end_offsets.push(key_data.len() as u16);
        }

        let mut key = Vec::new();
        for off in &end_offsets[..end_offsets.len() - 1] {
            key.extend_from_slice(&off.to_be_bytes());
        }
        key.extend_from_slice(&key_data);

        // Encode non-PK columns into value.
        let mut val = Vec::new();
        for (i, v) in values.iter().enumerate() {
            if !self.pk_positions.contains(&i) {
                encode_value(v, &mut val);
            }
        }

        (RowKey(key), RowValue(val))
    }

    /// Decode key + value back into a full row of Values (schema order).
    pub fn decode(&self, key: &RowKey, value: &RowValue, schema: &Schema) -> Vec<FieldValue> {
        let n = self.pk_positions.len();
        let total = schema.fields.len();

        // Decode PK columns from key.
        let offset_array_size = (n - 1) * 2;
        let key_bytes = &key.0;
        let col_data = &key_bytes[offset_array_size..];

        let mut end_offsets = Vec::with_capacity(n);
        for i in 0..n - 1 {
            let off = u16::from_be_bytes([key_bytes[i * 2], key_bytes[i * 2 + 1]]);
            end_offsets.push(off as usize);
        }
        end_offsets.push(col_data.len());

        let mut pk_values = Vec::with_capacity(n);
        let mut start = 0;
        for (i, &pos) in self.pk_positions.iter().enumerate() {
            let end = end_offsets[i];
            pk_values.push((pos, decode_value(&col_data[start..end], &schema.fields[pos].data_type)));
            start = end;
        }

        // Decode non-PK columns from value.
        let val_bytes = &value.0;
        let mut non_pk_values = Vec::new();
        let mut pos = 0;
        for (i, field) in schema.fields.iter().enumerate() {
            if !self.pk_positions.contains(&i) {
                let (v, consumed) = decode_value_with_len(&val_bytes[pos..], &field.data_type);
                non_pk_values.push((i, v));
                pos += consumed;
            }
        }

        // Reassemble in schema order.
        let mut result = vec![FieldValue::Null; total];
        for (idx, v) in pk_values {
            result[idx] = v;
        }
        for (idx, v) in non_pk_values {
            result[idx] = v;
        }
        result
    }

    /// Read a single PK column from the key without full decode.
    pub fn read_pk_column<'a>(&self, key: &'a RowKey, col_index: usize) -> &'a [u8] {
        let n = self.pk_positions.len();
        let offset_array_size = (n - 1) * 2;
        let key_bytes = &key.0;
        let col_data = &key_bytes[offset_array_size..];

        let start = if col_index == 0 {
            0
        } else {
            let off_pos = (col_index - 1) * 2;
            u16::from_be_bytes([key_bytes[off_pos], key_bytes[off_pos + 1]]) as usize
        };

        let end = if col_index < n - 1 {
            let off_pos = col_index * 2;
            u16::from_be_bytes([key_bytes[off_pos], key_bytes[off_pos + 1]]) as usize
        } else {
            col_data.len()
        };

        &col_data[start..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::decode_value;
    use expr::schema::{Field, Schema};
    use expr::types::{DataType, FieldValue};

    fn employee_schema() -> Schema {
        Schema::new(vec![
            Field::new("dept_id", DataType::Int),
            Field::new("emp_id", DataType::Int),
            Field::new("name", DataType::Str),
            Field::new("salary", DataType::Int),
            Field::new("email", DataType::Str),
            Field::new("active", DataType::Bool),
        ])
    }

    #[test]
    fn roundtrip_encode_decode() {
        let schema = employee_schema();
        let codec = RowCodec::new(3);
        let row = vec![
            FieldValue::Int(10),
            FieldValue::Int(42),
            FieldValue::Str("alice".into()),
            FieldValue::Int(95000),
            FieldValue::Str("alice@co.com".into()),
            FieldValue::Bool(true),
        ];

        let (key, val) = codec.encode(&row);
        let decoded = codec.decode(&key, &val, &schema);
        assert_eq!(decoded, row);
    }

    #[test]
    fn single_pk_column() {
        let schema = Schema::new(vec![
            Field::new("id", DataType::Int),
            Field::new("name", DataType::Str),
        ]);
        let codec = RowCodec::new(1);
        let row = vec![FieldValue::Int(7), FieldValue::Str("bob".into())];

        let (key, val) = codec.encode(&row);
        let decoded = codec.decode(&key, &val, &schema);
        assert_eq!(decoded, row);
    }

    #[test]
    fn pk_not_first_columns() {
        // PK is column 2 ("email"), not the first column.
        let schema = Schema::new(vec![
            Field::new("name", DataType::Str),
            Field::new("age", DataType::Int),
            Field::new("email", DataType::Str),
        ]);
        let codec = RowCodec::from_pk_positions(vec![2]);
        let row = vec![
            FieldValue::Str("alice".into()),
            FieldValue::Int(30),
            FieldValue::Str("alice@co.com".into()),
        ];

        let (key, val) = codec.encode(&row);
        let decoded = codec.decode(&key, &val, &schema);
        assert_eq!(decoded, row);
    }

    #[test]
    fn composite_pk_non_contiguous() {
        // PK is columns 0 and 2.
        let schema = Schema::new(vec![
            Field::new("dept", DataType::Int),
            Field::new("name", DataType::Str),
            Field::new("emp_id", DataType::Int),
        ]);
        let codec = RowCodec::from_pk_positions(vec![0, 2]);
        let row = vec![
            FieldValue::Int(10),
            FieldValue::Str("alice".into()),
            FieldValue::Int(42),
        ];

        let (key, val) = codec.encode(&row);
        let decoded = codec.decode(&key, &val, &schema);
        assert_eq!(decoded, row);
    }

    #[test]
    fn read_pk_column_direct() {
        let codec = RowCodec::new(3);
        let row = vec![
            FieldValue::Int(10),
            FieldValue::Int(42),
            FieldValue::Str("alice".into()),
            FieldValue::Int(95000),
            FieldValue::Str("alice@co.com".into()),
            FieldValue::Bool(true),
        ];

        let (key, _) = codec.encode(&row);

        let col0 = codec.read_pk_column(&key, 0);
        assert_eq!(decode_value(col0, &DataType::Int), FieldValue::Int(10));

        let col1 = codec.read_pk_column(&key, 1);
        assert_eq!(decode_value(col1, &DataType::Int), FieldValue::Int(42));

        let col2 = codec.read_pk_column(&key, 2);
        assert_eq!(decode_value(col2, &DataType::Str), FieldValue::Str("alice".into()));
    }

    #[test]
    fn null_values() {
        let schema = Schema::new(vec![
            Field::new("id", DataType::Int),
            Field::new("val", DataType::Str),
        ]);
        let codec = RowCodec::new(1);
        let row = vec![FieldValue::Int(1), FieldValue::Null];

        let (key, val) = codec.encode(&row);
        let decoded = codec.decode(&key, &val, &schema);
        assert_eq!(decoded, row);
    }

    #[test]
    fn offset_array_size() {
        let codec = RowCodec::new(3);
        let row = vec![
            FieldValue::Int(1), FieldValue::Int(2), FieldValue::Str("x".into()),
            FieldValue::Bool(false),
        ];
        let (key, _) = codec.encode(&row);
        // 2 offsets × 2 bytes = 4, col0: 9, col1: 9, col2: 4 = 26
        assert_eq!(key.0.len(), 26);
    }
}
