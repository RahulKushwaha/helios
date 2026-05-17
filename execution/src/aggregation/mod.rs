pub mod hash_aggregate;
pub mod scalar_aggregate;
pub mod sort_aggregate;

use expr::schema::Schema;

use crate::engine::ExecutionError;
use crate::stream::{Batch, RowStream};

/// Strategy abstraction for aggregation.
///
/// An `Aggregator` consumes input rows in batches via `accumulate` and
/// emits result rows via `finalize` (and optionally during `accumulate` for
/// streaming strategies like sort-based aggregation).
///
/// Wrap an `Aggregator` in [`AggregateStream`] to plug it into the operator
/// pipeline as a [`RowStream`].
///
/// Each implementation owns the input schema and the expressions it needs;
/// the trait surface stays minimal.
pub trait Aggregator {
    /// Feed one batch of input rows.
    ///
    /// Streaming strategies (sort-based) may emit completed groups here as
    /// soon as the group key changes. Blocking strategies (hash, scalar)
    /// buffer state and return `Ok(None)` until `finalize`.
    ///
    /// Convention: never return `Some(empty_batch)`. If there's nothing to
    /// emit, return `Ok(None)`.
    fn accumulate(&mut self, batch: &Batch) -> Result<Option<Batch>, ExecutionError>;

    /// Called repeatedly after the input is exhausted to drain any buffered
    /// output. Returns `Ok(None)` once everything has been emitted.
    ///
    /// Implementations may emit all buffered rows in one batch or chunk them
    /// across multiple calls — the surrounding `AggregateStream` keeps calling
    /// until it sees `None`.
    fn finalize(&mut self) -> Result<Option<Batch>, ExecutionError>;
}

/// Adapts any `Aggregator` to the operator pipeline by implementing
/// [`RowStream`]. Pulls batches from `input`, feeds them to the aggregator,
/// and once the input is exhausted drains the aggregator's `finalize` calls.
///
/// Holds the *output* schema (group columns followed by aggregate result
/// columns) — derived at construction by the caller, since it depends on
/// both the strategy and the input schema.
pub struct AggregateStream<'a> {
    input: Box<dyn RowStream + 'a>,
    aggregator: Box<dyn Aggregator + 'a>,
    output_schema: Schema,
    /// Set to `true` once the input stream has returned `None`. After that,
    /// `next_batch` only calls `aggregator.finalize` until it also returns `None`.
    input_exhausted: bool,
}

impl<'a> AggregateStream<'a> {
    pub fn new(
        input: Box<dyn RowStream + 'a>,
        aggregator: Box<dyn Aggregator + 'a>,
        output_schema: Schema,
    ) -> Self {
        Self {
            input,
            aggregator,
            output_schema,
            input_exhausted: false,
        }
    }
}

impl<'a> RowStream for AggregateStream<'a> {
    fn next_batch(&mut self) -> Result<Option<Batch>, ExecutionError> {
        // Phase 2: input drained, drain the aggregator until empty.
        if self.input_exhausted {
            return self.aggregator.finalize();
        }

        // Phase 1: pull from input. Loop because some batches won't produce
        // output (blocking aggregators always return None during accumulate).
        loop {
            match self.input.next_batch()? {
                Some(batch) => {
                    if let Some(out) = self.aggregator.accumulate(&batch)? {
                        debug_assert!(
                            !out.is_empty(),
                            "Aggregator::accumulate must not return Some(empty)"
                        );
                        return Ok(Some(out));
                    }
                    // No output for this batch — pull more.
                }
                None => {
                    self.input_exhausted = true;
                    return self.aggregator.finalize();
                }
            }
        }
    }

    fn schema(&self) -> &Schema {
        &self.output_schema
    }
}
