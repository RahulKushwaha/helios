use expr::expr::Expr;
use expr::schema::Schema;
use expr::types::FieldValue;

use crate::aggregation::Aggregator;
use crate::engine::{ExecutionError, ResultSet, Row};
use crate::evaluator::eval;
use crate::helpers::compute_aggregate;
use crate::stream::Batch;

/// Scalar aggregation: no GROUP BY, the entire input is one group.
/// Returns exactly one row. O(n) time, O(1) memory (no hash table).
pub fn execute_scalar_aggregate(
    rows: &[Row],
    schema: &Schema,
    aggr_exprs: &[Expr],
) -> Result<ResultSet, ExecutionError> {
    let mut out_row = Vec::new();
    for agg_expr in aggr_exprs {
        let val = match agg_expr {
            Expr::AggregateFunction { fun, args } => {
                let vals: Vec<FieldValue> = rows
                    .iter()
                    .map(|row| {
                        args.first()
                            .map(|a| eval(a, row, schema).unwrap_or(FieldValue::Null))
                            .unwrap_or(FieldValue::Null)
                    })
                    .collect();
                let refs: Vec<&FieldValue> = vals.iter().collect();
                compute_aggregate(fun, &refs)
            }
            _ => FieldValue::Null,
        };
        out_row.push(val);
    }

    Ok(vec![out_row])
}

/// Scalar aggregation as an `Aggregator`. No GROUP BY: the entire input is a
/// single group; emits exactly one row.
///
/// **Pipeline type:** blocking (one row out, only after input drains).
/// **Memory:** O(1) — per-aggregate accumulator state, no hash table.
pub struct ScalarAggregator {
    pub input_schema: Schema,
    pub aggr_exprs: Vec<Expr>,
    // TODO: add per-aggregate accumulator state. Two reasonable options:
    //   (a) Buffer all rows and compute aggregates in finalize (matches the
    //       existing free function above; simple but O(n) memory).
    //   (b) Maintain incremental accumulators (e.g. running sum + count) so
    //       memory stays O(1). Preferred since this is the whole point of
    //       a dedicated scalar variant.
}

impl ScalarAggregator {
    pub fn new(input_schema: Schema, aggr_exprs: Vec<Expr>) -> Self {
        Self {
            input_schema,
            aggr_exprs,
        }
    }
}

impl Aggregator for ScalarAggregator {
    fn accumulate(&mut self, _batch: &Batch) -> Result<Option<Batch>, ExecutionError> {
        // TODO: Update the running accumulators (or buffer rows) for each
        // aggregate function. No output during accumulation: return Ok(None).
        todo!("ScalarAggregator::accumulate")
    }

    fn finalize(&mut self) -> Result<Option<Batch>, ExecutionError> {
        // TODO: First call: produce the single output row from the
        // accumulators and return Ok(Some(vec![row])). Subsequent calls:
        // return Ok(None).
        //
        // Note: SQL aggregates over an empty input still produce a row with
        // sentinel values (COUNT=0, SUM=NULL, MIN/MAX=NULL). The existing
        // free function above shows the expected shape.
        todo!("ScalarAggregator::finalize")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use expr::expr::{AggFunc, Expr};
    use expr::schema::Field;
    use expr::types::DataType;

    fn sample_schema() -> Schema {
        Schema::new(vec![
            Field::new("id", DataType::Int),
            Field::new("score", DataType::Int),
        ])
    }

    fn sample_rows() -> Vec<Row> {
        vec![
            vec![FieldValue::Int(1), FieldValue::Int(90)],
            vec![FieldValue::Int(2), FieldValue::Int(75)],
            vec![FieldValue::Int(3), FieldValue::Int(60)],
        ]
    }

    #[test]
    fn test_count() {
        let result = execute_scalar_aggregate(
            &sample_rows(),
            &sample_schema(),
            &[Expr::AggregateFunction {
                fun: AggFunc::Count,
                args: vec![Expr::Column("id".into())],
            }],
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], vec![FieldValue::Int(3)]);
    }

    #[test]
    fn test_sum() {
        let result = execute_scalar_aggregate(
            &sample_rows(),
            &sample_schema(),
            &[Expr::AggregateFunction {
                fun: AggFunc::Sum,
                args: vec![Expr::Column("score".into())],
            }],
        )
        .unwrap();
        assert_eq!(result[0], vec![FieldValue::Int(225)]);
    }

    #[test]
    fn test_min_max() {
        let result = execute_scalar_aggregate(
            &sample_rows(),
            &sample_schema(),
            &[
                Expr::AggregateFunction {
                    fun: AggFunc::Min,
                    args: vec![Expr::Column("score".into())],
                },
                Expr::AggregateFunction {
                    fun: AggFunc::Max,
                    args: vec![Expr::Column("score".into())],
                },
            ],
        )
        .unwrap();
        assert_eq!(result[0], vec![FieldValue::Int(60), FieldValue::Int(90)]);
    }

    #[test]
    fn test_avg() {
        let result = execute_scalar_aggregate(
            &sample_rows(),
            &sample_schema(),
            &[Expr::AggregateFunction {
                fun: AggFunc::Avg,
                args: vec![Expr::Column("score".into())],
            }],
        )
        .unwrap();
        assert_eq!(result[0], vec![FieldValue::Float(75.0)]);
    }

    #[test]
    fn test_empty_input() {
        let result = execute_scalar_aggregate(
            &[],
            &sample_schema(),
            &[Expr::AggregateFunction {
                fun: AggFunc::Count,
                args: vec![Expr::Column("id".into())],
            }],
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], vec![FieldValue::Int(0)]);
    }

    #[test]
    fn test_multiple_aggregates() {
        let result = execute_scalar_aggregate(
            &sample_rows(),
            &sample_schema(),
            &[
                Expr::AggregateFunction {
                    fun: AggFunc::Count,
                    args: vec![Expr::Column("id".into())],
                },
                Expr::AggregateFunction {
                    fun: AggFunc::Sum,
                    args: vec![Expr::Column("score".into())],
                },
                Expr::AggregateFunction {
                    fun: AggFunc::Avg,
                    args: vec![Expr::Column("score".into())],
                },
            ],
        )
        .unwrap();
        assert_eq!(
            result[0],
            vec![FieldValue::Int(3), FieldValue::Int(225), FieldValue::Float(75.0)]
        );
    }
}
