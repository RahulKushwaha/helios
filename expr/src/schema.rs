use crate::types::DataType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single column definition with storage metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub data_type: DataType,
    #[serde(default = "default_true")]
    pub nullable: bool,
    #[serde(default)]
    pub is_pk: bool,
    #[serde(default)]
    pub col_pos: usize,
}

fn default_true() -> bool { true }

impl Column {
    pub fn new(name: impl Into<String>, data_type: DataType) -> Self {
        Self {
            name: name.into(),
            data_type,
            nullable: true,
            is_pk: false,
            col_pos: 0,
        }
    }

    pub fn with_pk(mut self, is_pk: bool) -> Self {
        self.is_pk = is_pk;
        self
    }

    pub fn with_nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }

    pub fn with_pos(mut self, col_pos: usize) -> Self {
        self.col_pos = col_pos;
        self
    }
}

/// Backward compatibility alias.
pub type Field = Column;

/// Ordered list of columns describing the shape of a relation (table or intermediate result).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schema {
    pub fields: Vec<Column>,
    #[serde(skip)]
    pub lookup: HashMap<String, usize>,
}

impl Schema {
    pub fn new(fields: Vec<Column>) -> Self {
        let mut lookup = HashMap::new();
        for field in fields.iter().enumerate() {
            lookup.insert(field.1.name.clone(), lookup.len());
        }
        Self { fields, lookup }
    }

    /// Rebuild the lookup map from fields. Call after deserialization.
    pub fn rebuild_lookup(&mut self) {
        self.lookup.clear();
        for (i, f) in self.fields.iter().enumerate() {
            self.lookup.insert(f.name.clone(), i);
        }
    }

    /// Look up a field by name. Returns the index and a reference to the Column.
    pub fn field_by_name(&self, name: &str) -> Option<(usize, &Column)> {
        self.lookup.get(name).map(|idx| (*idx, &self.fields[*idx]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_schema() -> Schema {
        Schema::new(vec![
            Column::new("id", DataType::Int),
            Column::new("name", DataType::Str),
            Column::new("active", DataType::Bool),
        ])
    }

    #[test]
    fn column_new_defaults() {
        let c = Column::new("age", DataType::Int);
        assert_eq!(c.name, "age");
        assert_eq!(c.data_type, DataType::Int);
        assert!(c.nullable);
        assert!(!c.is_pk);
        assert_eq!(c.col_pos, 0);
    }

    #[test]
    fn column_builder_methods() {
        let c = Column::new("id", DataType::Int)
            .with_pk(true)
            .with_nullable(false)
            .with_pos(0);
        assert!(c.is_pk);
        assert!(!c.nullable);
        assert_eq!(c.col_pos, 0);
    }

    #[test]
    fn field_alias_works() {
        let f = Field::new("x", DataType::Float);
        assert_eq!(f.name, "x");
    }

    #[test]
    fn field_clone_eq() {
        let f = Column::new("x", DataType::Float);
        assert_eq!(f.clone(), f);
        assert_ne!(f, Column::new("y", DataType::Float));
    }

    #[test]
    fn schema_stores_fields_in_order() {
        let s = sample_schema();
        assert_eq!(s.fields.len(), 3);
        assert_eq!(s.fields[0].name, "id");
        assert_eq!(s.fields[2].name, "active");
    }

    #[test]
    fn schema_empty() {
        let s = Schema::new(vec![]);
        assert!(s.fields.is_empty());
        assert!(s.lookup.is_empty());
    }

    #[test]
    fn field_by_name_found() {
        let s = sample_schema();
        let (idx, col) = s.field_by_name("name").unwrap();
        assert_eq!(idx, 1);
        assert_eq!(col.data_type, DataType::Str);
    }

    #[test]
    fn field_by_name_first_field() {
        let s = sample_schema();
        let (idx, col) = s.field_by_name("id").unwrap();
        assert_eq!(idx, 0);
        assert_eq!(col.data_type, DataType::Int);
    }

    #[test]
    fn field_by_name_last_field() {
        let s = sample_schema();
        let (idx, _) = s.field_by_name("active").unwrap();
        assert_eq!(idx, 2);
    }

    #[test]
    fn field_by_name_missing() {
        let s = sample_schema();
        assert!(s.field_by_name("nonexistent").is_none());
    }

    #[test]
    fn field_by_name_empty_schema() {
        let s = Schema::new(vec![]);
        assert!(s.field_by_name("anything").is_none());
    }

    #[test]
    fn lookup_has_all_fields() {
        let s = sample_schema();
        assert_eq!(s.lookup.len(), 3);
        assert_eq!(s.lookup["id"], 0);
        assert_eq!(s.lookup["name"], 1);
        assert_eq!(s.lookup["active"], 2);
    }

    #[test]
    fn schema_clone_eq() {
        let s = sample_schema();
        assert_eq!(s.clone(), s);
    }
}
