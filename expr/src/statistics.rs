// [File 12] Statistics structs for cost estimation
//
// DataFusion ref: datafusion/common/src/stats.rs

use crate::types::FieldValue;

/// Per-column statistics. Fields are Option so you can start with row counts
/// and add histograms / min-max later without breaking changes.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnStatistics {
    pub distinct_count: Option<usize>,
    pub min_value: Option<FieldValue>,
    pub max_value: Option<FieldValue>,
}

/// Table-level statistics used by the cost-based optimizer.
#[derive(Debug, Clone, PartialEq)]
pub struct Statistics {
    pub row_count: Option<usize>,
    pub column_statistics: Vec<ColumnStatistics>,
}

impl Default for ColumnStatistics {
    fn default() -> Self {
        Self {
            distinct_count: None,
            min_value: None,
            max_value: None,
        }
    }
}

impl Default for Statistics {
    fn default() -> Self {
        Self {
            row_count: None,
            column_statistics: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_statistics() {
        // TODO: Create Statistics with known row_count and column stats,
        // verify fields are set correctly
        todo!()
    }

    #[test]
    fn test_statistics_defaults() {
        // TODO: Verify Statistics::default() has None row_count and empty column_statistics
        todo!()
    }
}
