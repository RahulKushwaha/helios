//! Pull-based, batched (Volcano-style) execution streams.
//!
//! Each operator implements [`RowStream`], yielding a `Batch` of rows per
//! `next_batch()` call. Batching amortizes per-call overhead — virtual dispatch,
//! schema lookup, predicate setup — over many rows.
//!
//! Tuned for OLTP: the default batch size is small (`DEFAULT_BATCH_SIZE`) so
//! point queries and short range scans don't pay a fat-batch latency penalty.
//! For analytical workloads, you'd want larger batches and a columnar payload.
//!
//! [`build_stream`] constructs the iterator tree from a [`PhysicalPlan`].
//! Operators not yet converted to streaming fall back to [`MaterializedStream`].

use expr::schema::Schema;
use physical_plan::plan::PhysicalPlan;

use crate::aggregation::scalar_aggregate::ScalarAggregator;
use crate::aggregation::sort_aggregate::SortAggregator;
use crate::aggregation::AggregateStream;
use crate::engine::{DataSource, ExecutionError, Row};
use crate::helpers::plan_schema;
use crate::materialized::execute_plan;

/// A chunk of rows passed between operators.
pub type Batch = Vec<Row>;

/// Default batch size (rows). Small for OLTP — most queries return few rows.
pub const DEFAULT_BATCH_SIZE: usize = 1024;

/// A pull-based row producer. `next_batch` returns the next non-empty batch
/// of rows, or `None` when the stream is exhausted.
///
/// Convention: streams never return `Some(empty_batch)`. They keep pulling
/// internally and only return `None` when truly done.
pub trait RowStream {
    fn next_batch(&mut self) -> Result<Option<Batch>, ExecutionError>;
    fn schema(&self) -> &Schema;
}

/// Build an iterator tree from a `PhysicalPlan`.
///
/// As each operator is converted to streaming, add an arm above the fallback.
pub fn build_stream<'a>(
    plan: &PhysicalPlan,
    source: &'a dyn DataSource,
) -> Result<Box<dyn RowStream + 'a>, ExecutionError> {
    match plan {
        PhysicalPlan::Limit { skip, fetch, input } => {
            let input_stream = build_stream(input, source)?;
            Ok(Box::new(LimitStream::new(input_stream, *skip, *fetch)))
        }

        PhysicalPlan::ScalarAggregate { aggr_exprs, input } => {
            let input_stream = build_stream(input, source)?;
            let input_schema = input_stream.schema().clone();
            let output_schema = plan_schema(plan);
            let aggregator = ScalarAggregator::new(input_schema, aggr_exprs.clone());
            Ok(Box::new(AggregateStream::new(
                input_stream,
                Box::new(aggregator),
                output_schema,
            )))
        }

        PhysicalPlan::SortAggregate {
            group_by,
            aggr_exprs,
            input,
        } => {
            let input_stream = build_stream(input, source)?;
            let input_schema = input_stream.schema().clone();
            let output_schema = plan_schema(plan);
            let aggregator =
                SortAggregator::new(input_schema, group_by.clone(), aggr_exprs.clone());
            Ok(Box::new(AggregateStream::new(
                input_stream,
                Box::new(aggregator),
                output_schema,
            )))
        }

        // HashAggregate intentionally falls through to the materialized
        // fallback below. Add an arm here that constructs HashAggregator +
        // AggregateStream once you've implemented its accumulate/finalize.

        // Fallback: materialize via execute_plan and chunk into batches.
        other => {
            let rows = execute_plan(source, other)?;
            let schema = plan_schema(other);
            Ok(Box::new(MaterializedStream::new(rows, schema)))
        }
    }
}

/// A stream backed by a fully materialized `Vec<Row>`, served in batches.
/// Used as the migration fallback and as a building block in tests.
pub struct MaterializedStream {
    rows: std::vec::IntoIter<Row>,
    batch_size: usize,
    schema: Schema,
}

impl MaterializedStream {
    pub fn new(rows: Vec<Row>, schema: Schema) -> Self {
        Self::with_batch_size(rows, schema, DEFAULT_BATCH_SIZE)
    }

    pub fn with_batch_size(rows: Vec<Row>, schema: Schema, batch_size: usize) -> Self {
        debug_assert!(batch_size > 0);
        Self {
            rows: rows.into_iter(),
            batch_size,
            schema,
        }
    }
}

impl RowStream for MaterializedStream {
    fn next_batch(&mut self) -> Result<Option<Batch>, ExecutionError> {
        let batch: Batch = (&mut self.rows).take(self.batch_size).collect();
        if batch.is_empty() {
            Ok(None)
        } else {
            Ok(Some(batch))
        }
    }

    fn schema(&self) -> &Schema {
        &self.schema
    }
}

/// LIMIT / OFFSET as a streaming, batched operator.
///
/// 1. Drains the first `skip` rows from the input across one or more batches.
/// 2. Yields up to `fetch` rows, slicing batches at boundaries when needed.
/// 3. Stops pulling from the child as soon as `fetch` rows have been emitted.
pub struct LimitStream<'a> {
    input: Box<dyn RowStream + 'a>,
    skip: usize,
    fetch: usize,
    skipped: usize,
    emitted: usize,
    done: bool,
}

impl<'a> LimitStream<'a> {
    pub fn new(input: Box<dyn RowStream + 'a>, skip: usize, fetch: usize) -> Self {
        Self {
            input,
            skip,
            fetch,
            skipped: 0,
            emitted: 0,
            done: false,
        }
    }

    /// Apply `fetch` to a batch we're ready to emit. Returns `Some` if any
    /// rows survive, `None` if the batch is empty after truncation.
    fn emit(&mut self, mut batch: Batch) -> Option<Batch> {
        let fetch_remaining = self.fetch - self.emitted;
        if fetch_remaining == 0 {
            self.done = true;
            return None;
        }
        let take = batch.len().min(fetch_remaining);
        batch.truncate(take);
        self.emitted += take;
        if self.emitted >= self.fetch {
            self.done = true;
        }
        if batch.is_empty() {
            None
        } else {
            Some(batch)
        }
    }
}

impl<'a> RowStream for LimitStream<'a> {
    fn next_batch(&mut self) -> Result<Option<Batch>, ExecutionError> {
        if self.done {
            return Ok(None);
        }

        loop {
            let batch = match self.input.next_batch()? {
                None => {
                    self.done = true;
                    return Ok(None);
                }
                Some(b) => b,
            };

            // Phase 1: still skipping. Discard or partially discard this batch.
            if self.skipped < self.skip {
                let need = self.skip - self.skipped;
                if need >= batch.len() {
                    self.skipped += batch.len();
                    continue;
                }
                self.skipped += need;
                let remainder: Batch = batch.into_iter().skip(need).collect();
                if let Some(out) = self.emit(remainder) {
                    return Ok(Some(out));
                }
                // emit() may have returned None (fetch=0 or batch empty after
                // truncation). Loop and try again.
                if self.done {
                    return Ok(None);
                }
                continue;
            }

            // Phase 2: emitting.
            if let Some(out) = self.emit(batch) {
                return Ok(Some(out));
            }
            if self.done {
                return Ok(None);
            }
        }
    }

    fn schema(&self) -> &Schema {
        self.input.schema()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use expr::schema::{Field, Schema};
    use expr::types::{DataType, FieldValue};
    use std::cell::Cell;
    use std::rc::Rc;

    fn row(i: i64) -> Row {
        vec![FieldValue::Int(i)]
    }

    fn int_schema() -> Schema {
        Schema::new(vec![Field::new("n", DataType::Int)])
    }

    fn drain<S: RowStream>(stream: &mut S) -> Vec<Row> {
        let mut out = Vec::new();
        while let Some(batch) = stream.next_batch().unwrap() {
            assert!(!batch.is_empty(), "streams must never yield empty batches");
            out.extend(batch);
        }
        out
    }

    #[test]
    fn materialized_stream_chunks_into_batches() {
        let mut s = MaterializedStream::with_batch_size(
            (1..=10).map(row).collect(),
            int_schema(),
            3,
        );
        let b1 = s.next_batch().unwrap().unwrap();
        let b2 = s.next_batch().unwrap().unwrap();
        let b3 = s.next_batch().unwrap().unwrap();
        let b4 = s.next_batch().unwrap().unwrap();
        assert_eq!(s.next_batch().unwrap(), None);

        assert_eq!(b1.len(), 3);
        assert_eq!(b2.len(), 3);
        assert_eq!(b3.len(), 3);
        assert_eq!(b4.len(), 1);
        assert_eq!(b4[0], row(10));
    }

    #[test]
    fn limit_skip_and_fetch_across_batches() {
        let inner = Box::new(MaterializedStream::with_batch_size(
            (1..=10).map(row).collect(),
            int_schema(),
            3, // forces skip and fetch to land mid-batch
        ));
        let mut limit = LimitStream::new(inner, 3, 4);
        assert_eq!(drain(&mut limit), vec![row(4), row(5), row(6), row(7)]);
    }

    #[test]
    fn limit_short_circuits_input() {
        // Counts how many batches the child stream actually serves.
        struct CountingStream {
            inner: MaterializedStream,
            batches_pulled: Rc<Cell<usize>>,
        }
        impl RowStream for CountingStream {
            fn next_batch(&mut self) -> Result<Option<Batch>, ExecutionError> {
                let result = self.inner.next_batch()?;
                if result.is_some() {
                    self.batches_pulled
                        .set(self.batches_pulled.get() + 1);
                }
                Ok(result)
            }
            fn schema(&self) -> &Schema {
                self.inner.schema()
            }
        }

        let pulls = Rc::new(Cell::new(0));
        // Source has 1000 rows in batches of 5, but fetch=5 should satisfy
        // from the first batch alone.
        let inner = Box::new(CountingStream {
            inner: MaterializedStream::with_batch_size(
                (1..=1000).map(row).collect(),
                int_schema(),
                5,
            ),
            batches_pulled: pulls.clone(),
        });

        let mut limit = LimitStream::new(inner, 0, 5);
        let got = drain(&mut limit);

        assert_eq!(got.len(), 5);
        assert_eq!(
            pulls.get(),
            1,
            "Limit should stop after one batch when fetch fits within it"
        );
    }

    #[test]
    fn limit_fetch_zero_returns_nothing() {
        let inner = Box::new(MaterializedStream::with_batch_size(
            (1..=10).map(row).collect(),
            int_schema(),
            4,
        ));
        let mut limit = LimitStream::new(inner, 0, 0);
        assert_eq!(drain(&mut limit), Vec::<Row>::new());
    }
}
