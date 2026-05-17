use std::cmp::Ordering;

use crate::encoding::decode_value_with_len;
use expr::types::DataType;

/// Encoded key: offset array followed by PK column bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowKey(pub Vec<u8>);

/// Encoded value: non-key column bytes packed sequentially.
#[derive(Debug, Clone, PartialEq)]
pub struct RowValue(pub Vec<u8>);

/// Compare two encoded values byte-by-byte, interpreting the tag byte
/// to apply correct ordering per type.
fn cmp_encoded_values(a: &[u8], b: &[u8]) -> Ordering {
    if a.is_empty() && b.is_empty() {
        return Ordering::Equal;
    }
    if a.is_empty() {
        return Ordering::Less;
    }
    if b.is_empty() {
        return Ordering::Greater;
    }

    // Tag byte determines type. Different types sort by tag order:
    // Null(0) < Bool(1) < Int(2) < Float(3) < Str(4)
    let tag_a = a[0];
    let tag_b = b[0];
    if tag_a != tag_b {
        return tag_a.cmp(&tag_b);
    }

    match tag_a {
        0 => Ordering::Equal, // both Null
        1 => a[1].cmp(&b[1]), // Bool: 0=false < 1=true
        2 => {
            // Int: decode as i64 and compare (raw bytes don't sort correctly for signed)
            let va = i64::from_be_bytes(a[1..9].try_into().unwrap());
            let vb = i64::from_be_bytes(b[1..9].try_into().unwrap());
            va.cmp(&vb)
        }
        3 => {
            // Float: decode and compare
            let va = f64::from_be_bytes(a[1..9].try_into().unwrap());
            let vb = f64::from_be_bytes(b[1..9].try_into().unwrap());
            va.partial_cmp(&vb).unwrap_or(Ordering::Equal)
        }
        4 => {
            // Str: compare length-prefixed strings
            let len_a = u16::from_be_bytes([a[1], a[2]]) as usize;
            let len_b = u16::from_be_bytes([b[1], b[2]]) as usize;
            a[3..3 + len_a].cmp(&b[3..3 + len_b])
        }
        _ => Ordering::Equal,
    }
}

impl RowKey {
    /// Compare two RowKeys column by column.
    /// `num_pk_cols` is needed to parse the offset array.
    pub fn compare(&self, other: &RowKey, num_pk_cols: usize) -> Ordering {
        let n = num_pk_cols;
        let offset_size = (n - 1) * 2;

        let a_data = &self.0[offset_size..];
        let b_data = &other.0[offset_size..];

        let a_ends = Self::read_end_offsets(&self.0, n);
        let b_ends = Self::read_end_offsets(&other.0, n);

        let mut a_start = 0;
        let mut b_start = 0;
        for i in 0..n {
            let a_end = a_ends[i];
            let b_end = b_ends[i];

            let ord = cmp_encoded_values(&a_data[a_start..a_end], &b_data[b_start..b_end]);
            if ord != Ordering::Equal {
                return ord;
            }

            a_start = a_end;
            b_start = b_end;
        }
        Ordering::Equal
    }

    fn read_end_offsets(key_bytes: &[u8], n: usize) -> Vec<usize> {
        let offset_size = (n - 1) * 2;
        let col_data_len = key_bytes.len() - offset_size;
        let mut ends = Vec::with_capacity(n);
        for i in 0..n - 1 {
            let off = u16::from_be_bytes([key_bytes[i * 2], key_bytes[i * 2 + 1]]);
            ends.push(off as usize);
        }
        ends.push(col_data_len);
        ends
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RowCodec;
    use expr::types::FieldValue;

    #[test]
    fn compare_single_int_pk() {
        let codec = RowCodec::new(1);
        let (k1, _) = codec.encode(&[FieldValue::Int(10), FieldValue::Str("a".into())]);
        let (k2, _) = codec.encode(&[FieldValue::Int(20), FieldValue::Str("b".into())]);
        let (k3, _) = codec.encode(&[FieldValue::Int(10), FieldValue::Str("c".into())]);

        assert_eq!(k1.compare(&k2, 1), Ordering::Less);
        assert_eq!(k2.compare(&k1, 1), Ordering::Greater);
        assert_eq!(k1.compare(&k3, 1), Ordering::Equal);
    }

    #[test]
    fn compare_negative_ints() {
        let codec = RowCodec::new(1);
        let (k_neg, _) = codec.encode(&[FieldValue::Int(-5)]);
        let (k_pos, _) = codec.encode(&[FieldValue::Int(5)]);
        let (k_zero, _) = codec.encode(&[FieldValue::Int(0)]);

        assert_eq!(k_neg.compare(&k_pos, 1), Ordering::Less);
        assert_eq!(k_neg.compare(&k_zero, 1), Ordering::Less);
        assert_eq!(k_zero.compare(&k_pos, 1), Ordering::Less);
    }

    #[test]
    fn compare_composite_pk() {
        let codec = RowCodec::new(2);
        let (k1, _) = codec.encode(&[FieldValue::Int(1), FieldValue::Str("b".into()), FieldValue::Int(0)]);
        let (k2, _) = codec.encode(&[FieldValue::Int(1), FieldValue::Str("a".into()), FieldValue::Int(0)]);
        let (k3, _) = codec.encode(&[FieldValue::Int(2), FieldValue::Str("a".into()), FieldValue::Int(0)]);

        // Same first col, second col decides: "b" > "a"
        assert_eq!(k1.compare(&k2, 2), Ordering::Greater);
        // Different first col: 1 < 2
        assert_eq!(k1.compare(&k3, 2), Ordering::Less);
    }

    #[test]
    fn compare_strings() {
        let codec = RowCodec::new(1);
        let (k1, _) = codec.encode(&[FieldValue::Str("apple".into())]);
        let (k2, _) = codec.encode(&[FieldValue::Str("banana".into())]);
        let (k3, _) = codec.encode(&[FieldValue::Str("apple".into())]);

        assert_eq!(k1.compare(&k2, 1), Ordering::Less);
        assert_eq!(k1.compare(&k3, 1), Ordering::Equal);
    }

    #[test]
    fn compare_null_ordering() {
        let codec = RowCodec::new(1);
        let (k_null, _) = codec.encode(&[FieldValue::Null]);
        let (k_int, _) = codec.encode(&[FieldValue::Int(1)]);

        // Null (tag 0) sorts before Int (tag 2)
        assert_eq!(k_null.compare(&k_int, 1), Ordering::Less);
    }
}
