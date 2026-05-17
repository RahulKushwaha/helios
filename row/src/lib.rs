//! InnoDB-inspired row format with key/value separation.
//!
//! Key layout: `[offset_array (N-1 × u16)] [pk_col_0] ... [pk_col_N-1]`
//! Value layout: `[non_key_col_0] [non_key_col_1] ...`

pub mod codec;
pub mod datarow;
pub mod encoding;
pub mod types;

pub use codec::RowCodec;
pub use datarow::DataRow;
pub use types::{RowKey, RowValue};
