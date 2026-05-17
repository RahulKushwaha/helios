# execution

The execution layer: evaluates physical plans and produces result rows. The engine is **pull-based and batched** (Volcano-style with row batches), tuned for OLTP — small batches keep point-query latency low while still amortizing per-call overhead.

## Public API: traits in `engine.rs`

```rust
pub trait ExecutionEngine {
    fn execute(&self, plan: &PhysicalPlan) -> Result<ResultSet, ExecutionError>;
}

pub trait DataSource {
    fn scan(&self, table_name: &str, schema: &Schema) -> Result<ResultSet, ExecutionError>;
}
```

`ExecutionEngine` is the entry point any caller (REPL, tests, distributed coordinator) uses. `DataSource` is the boundary to the storage layer — implement it to plug in any backend. The crate ships only the traits; concrete engines live elsewhere (`RocksEngine` in the `storage` crate).

A `ResultSet` is `Vec<Vec<FieldValue>>`.

## Streaming pipeline: `stream.rs`

The trait every operator implements:

```rust
pub trait RowStream {
    fn next_batch(&mut self) -> Result<Option<Batch>, ExecutionError>;
    fn schema(&self) -> &Schema;
}

pub type Batch = Vec<Row>;
pub const DEFAULT_BATCH_SIZE: usize = 1024;
```

Each `next_batch` call returns the next non-empty batch of rows or `None` when exhausted. By convention streams never yield `Some(empty)` — they keep pulling internally and only signal `None` when truly done.

`build_stream(plan, source)` constructs the iterator tree from a `PhysicalPlan`. The top-level `execute` builds the tree, drains it via `next_batch`, and concatenates the result.

### Operators

| Operator | Type | Notes |
|---|---|---|
| `LimitStream` | streaming | Skips `skip` rows then yields up to `fetch`; stops pulling from input as soon as `fetch` is satisfied. |
| `MaterializedStream` | adapter | Wraps a `Vec<Row>` and chunks it into batches. Used as the migration fallback for operators that haven't been converted to streaming. |
| `AggregateStream` | adapter | Wraps any `Aggregator` (see below) and produces a `RowStream`. |

Operators not yet converted to native streams (`Filter`, `Projection`, `Sort`, `NestedLoopJoin`, `HashAggregate`) fall through to `materialized.rs::execute_plan`, which executes them eagerly and hands back a `Vec<Row>`. `MaterializedStream` then wraps the result so the layer above sees a uniform interface. As each operator gets a streaming impl, it moves out of the fallback.

## Aggregation: `aggregation/`

A two-trait design separates *strategy* from *streaming*:

```rust
pub trait Aggregator {
    fn accumulate(&mut self, batch: &Batch) -> Result<Option<Batch>, ExecutionError>;
    fn finalize(&mut self) -> Result<Option<Batch>, ExecutionError>;
}
```

`accumulate` may emit completed groups inline (sort-based) or buffer state (hash, scalar). `finalize` is called repeatedly after input ends until it returns `None`, draining whatever was buffered. `AggregateStream` is the generic adapter that turns any `Aggregator` into a `RowStream`.

Three strategies live alongside the original free-function implementations:

- **`HashAggregator`** (`hash_aggregate.rs`) — blocking, O(group count) memory. General-purpose.
- **`SortAggregator`** (`sort_aggregate.rs`) — **streaming**, O(1) extra memory. Requires input pre-sorted on group keys; emits each completed group inline.
- **`ScalarAggregator`** (`scalar_aggregate.rs`) — blocking, O(1) memory. No GROUP BY.

Each is a struct with config fields and an `Aggregator` impl. The accumulate/finalize bodies are currently `todo!()` stubs; the existing `execute_*_aggregate` free functions in the same files show the row-level logic.

## Materialized executor: `materialized.rs`

`execute_plan(source, plan)` is the eager (non-streaming) executor used by the streaming fallback. It walks the plan recursively, fully materializing each operator's output into a `Vec<Row>` before passing it up. Storage-agnostic — reaches the leaf via the `DataSource` trait.

This module shrinks as more operators get streaming impls and disappears once every operator streams.

## Expression Evaluator: `evaluator.rs`

`eval(expr, row, schema)` evaluates an `Expr` against a single row:

- `Column(name)`: look up the column index, return the value.
- `Literal(v)`: return the value directly.
- `BinaryExpr`: recursively evaluate both sides, apply the operator (arithmetic, comparison, or logical). NULL propagates through arithmetic and comparisons.
- `AggregateFunction`: not evaluated per row; handled at the operator level.

`cmp_values` provides total ordering across `FieldValue` variants for use by `Sort` and the comparison operators.
