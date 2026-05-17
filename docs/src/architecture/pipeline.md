# Query Pipeline

Every SQL query flows through five stages before results reach the user.

## 1. Parsing

The `sql-parser` crate uses the `sqlparser` library to tokenize and parse SQL text. The `SqlPlanner` converts the parsed AST into one of:

- `SqlStatement::CreateTable` / `CreateIndex` / `Insert` for DDL and DML.
- `SqlStatement::Query(LogicalPlan)` for SELECT statements.

The planner resolves table names against a catalog (`HashMap<String, Schema>`) and maps SQL expressions to the `Expr` enum.

## 2. Logical Optimization

The `Optimizer` accepts a `LogicalPlan` and applies a list of `OptimizerRule` implementations in a fixed point loop (up to 16 passes). Each rule rewrites the plan tree, and the loop terminates early when no rule produces a change.

Categories of rules include:

- **Predicate pushdown**: move filters closer to scans.
- **Projection pruning**: remove unused columns early.
- **Join reordering**: reorder joins using cost estimates.
- **Constant folding / propagation**: evaluate constant expressions at plan time.
- **Aggregate optimization**: merge, eliminate, or push down aggregates.
- **Subquery decorrelation**: convert correlated subqueries to joins.

See the [optimizer crate reference](../crates/optimizer.md) for the full rule list.

## 3. Physical Planning

The `PhysicalPlanner` converts each `LogicalPlan` node to its physical counterpart:

| Logical | Physical |
|---|---|
| `Scan` | `TableScan` |
| `Filter` | `Filter` |
| `Projection` | `Projection` |
| `Join` | `NestedLoopJoin` |
| `Sort` | `Sort` |
| `Aggregate` | `HashAggregate` (default; `ScalarAggregate` and `SortAggregate` are wired but not yet selected by the planner) |
| `Limit` | `Limit` |

If the physical planner is not yet implemented for a node, `main.rs` falls back to a direct 1:1 mapping.

## 4. Execution

Any type implementing the `ExecutionEngine` trait can execute a `PhysicalPlan`. The default engine is **`RocksEngine`** (in the `storage` crate) — reads from RocksDB and supports index-accelerated scans when a secondary index exists.

Internally the engine uses a **pull-based, batched streaming pipeline**. Each operator implements `RowStream::next_batch`, yielding row batches lazily. `LimitStream` is the first native operator; the rest fall back to a materialized executor (`execution/src/materialized.rs`) that fully evaluates each subtree, with the result wrapped in `MaterializedStream` so the trait surface is uniform. As more operators get streaming impls, they move out of the fallback.

Aggregation goes through a separate `Aggregator` trait (with `accumulate`/`finalize`) wrapped by an `AggregateStream` adapter, so each strategy (Hash, Sort, Scalar) is its own struct that plugs into the same pipeline.

Expression evaluation against a single row is handled by `evaluator::eval`.

## 5. Result Display

`main.rs` derives column names from the plan's output schema, computes column widths, and prints a formatted table to stdout.

## EXPLAIN

Prefixing a SELECT with `EXPLAIN` skips execution and instead prints the logical and physical plan trees.
