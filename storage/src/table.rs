use expr::schema::Column;
use expr::statistics::Statistics;
use std::sync::{Arc, Mutex};

pub type TableId = u64;

/// Index metadata.
#[derive(Debug, Clone)]
pub struct Index {
    pub name: String,
    pub table_id: TableId,
    pub columns: Vec<String>,
}

/// Table struct for maintaining metadata about the table.
pub struct Table {
    pub id: TableId,
    inner: Arc<Mutex<TableInner>>,
}

struct TableInner {
    name: String,
    stats: Statistics,
    n_cols: usize,
    cols: Vec<Arc<Column>>,
    indexes: Vec<Arc<Index>>,
    next_row_id: u64,
}

impl Table {
    pub fn new(id: TableId, name: impl Into<String>, cols: Vec<Column>) -> Self {
        let name = name.into();
        let n_cols = cols.len();
        let cols: Vec<Arc<Column>> = cols.into_iter().map(Arc::new).collect();
        Self {
            id,
            inner: Arc::new(Mutex::new(TableInner {
                name,
                stats: Statistics::default(),
                n_cols,
                cols,
                indexes: Vec::new(),
                next_row_id: 0,
            })),
        }
    }

    pub fn name(&self) -> String {
        self.inner.lock().unwrap().name.clone()
    }

    pub fn n_cols(&self) -> usize {
        self.inner.lock().unwrap().n_cols
    }

    pub fn columns(&self) -> Vec<Arc<Column>> {
        self.inner.lock().unwrap().cols.clone()
    }

    pub fn pk_columns(&self) -> Vec<Arc<Column>> {
        self.inner.lock().unwrap().cols.iter().filter(|c| c.is_pk).cloned().collect()
    }

    pub fn num_pk_cols(&self) -> usize {
        self.inner.lock().unwrap().cols.iter().filter(|c| c.is_pk).count()
    }

    pub fn indexes(&self) -> Vec<Arc<Index>> {
        self.inner.lock().unwrap().indexes.clone()
    }

    pub fn has_index(&self, column: &str) -> bool {
        self.inner.lock().unwrap().indexes.iter().any(|idx| idx.columns.contains(&column.to_string()))
    }

    pub fn schema(&self) -> expr::schema::Schema {
        let inner = self.inner.lock().unwrap();
        let fields: Vec<Column> = inner.cols.iter().map(|c| (**c).clone()).collect();
        expr::schema::Schema::new(fields)
    }

    pub fn add_index(&self, index: Index) {
        self.inner.lock().unwrap().indexes.push(Arc::new(index));
    }

    pub fn stats(&self) -> Statistics {
        self.inner.lock().unwrap().stats.clone()
    }

    pub fn set_stats(&self, stats: Statistics) {
        self.inner.lock().unwrap().stats = stats;
    }

    pub fn next_row_id(&self) -> u64 {
        self.inner.lock().unwrap().next_row_id
    }

    pub fn increment_row_id(&self) -> u64 {
        let mut inner = self.inner.lock().unwrap();
        let id = inner.next_row_id;
        inner.next_row_id += 1;
        id
    }

    pub fn set_next_row_id(&self, id: u64) {
        self.inner.lock().unwrap().next_row_id = id;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use expr::types::DataType;

    #[test]
    fn table_basic() {
        let cols = vec![
            Column::new("id", DataType::Int).with_pk(true).with_pos(0),
            Column::new("name", DataType::Str).with_pos(1),
            Column::new("score", DataType::Int).with_pos(2),
        ];
        let table = Table::new(1, "users", cols);

        assert_eq!(table.id, 1);
        assert_eq!(table.name(), "users");
        assert_eq!(table.n_cols(), 3);
        assert_eq!(table.num_pk_cols(), 1);
        assert_eq!(table.pk_columns()[0].name, "id");
    }

    #[test]
    fn table_row_id_increment() {
        let table = Table::new(1, "t", vec![Column::new("x", DataType::Int).with_pk(true)]);
        assert_eq!(table.increment_row_id(), 0);
        assert_eq!(table.increment_row_id(), 1);
        assert_eq!(table.next_row_id(), 2);
    }

    #[test]
    fn table_add_index() {
        let table = Table::new(1, "t", vec![Column::new("x", DataType::Int).with_pk(true)]);
        table.add_index(Index {
            name: "idx_t_x".into(),
            table_id: 1,
            columns: vec!["x".into()],
        });
        assert_eq!(table.indexes().len(), 1);
        assert_eq!(table.indexes()[0].name, "idx_t_x");
    }
}
