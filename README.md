# helios

A from-scratch SQL query engine in Rust, built as a learning project. Implements the full pipeline — parser → logical plan → optimizer → physical plan → batched pull-based execution → RocksDB storage — and ships as an interactive REPL.

## Status

Under active development. The single-node path (parse → optimize → execute → return rows) is functional for a useful subset of SQL: SELECT with WHERE, JOIN, GROUP BY, ORDER BY, LIMIT/OFFSET, INSERT, CREATE TABLE, CREATE INDEX. Many optimizer rules contain `todo!()` stubs marking planned work.

## Quick start

```bash
cargo build
cargo run                  # opens the REPL with default data dir ./helios_data
cargo run -- /tmp/mydb     # custom data directory
```

Inside the REPL:

```sql
CREATE TABLE users (id INT PRIMARY KEY, name VARCHAR, score INT);
INSERT INTO users VALUES (1, 'alice', 90), (2, 'bob', 75), (3, 'carol', 90);

SELECT name, score FROM users WHERE score > 80 ORDER BY score DESC LIMIT 5;

CREATE INDEX ON users (score);
SELECT * FROM users WHERE score = 90;             -- uses the index

EXPLAIN SELECT name FROM users WHERE score > 80;  -- shows logical + physical plans
```

Dot commands: `.tables`, `.schema <table>`, `.quit`.

## Architecture

Cargo workspace with seven crates layered top-to-bottom:

| Crate | Role |
|---|---|
| `expr` | `FieldValue`, `DataType`, `Schema`, `Expr`, `LogicalPlan`, `Statistics` |
| `sql-parser` | SQL → `LogicalPlan` via the `sqlparser` crate |
| `optimizer` | Rule-based + cost-based rewrites over `LogicalPlan` |
| `physical-plan` | `PhysicalPlan` enum and logical→physical conversion |
| `execution` | Pull-based batched `RowStream` trait, `Aggregator` trait, expression evaluator |
| `storage` | RocksDB-backed tables and secondary indexes; `RocksEngine` |
| `row` | Row encoding and codec primitives |

Plus a `repl/` member crate that owns the binary entry point.

The execution layer is **pull-based with row batches** (Volcano-style). Each operator is a `RowStream`; `LimitStream` is the first native streaming operator (the rest fall back to a materialized executor and migrate over time). Aggregation uses an `Aggregator` trait with three strategies (Hash, Sort, Scalar) wrapped by a generic `AggregateStream` adapter.

## Documentation

Full docs live in [`docs/`](./docs) as an mdBook:

```bash
cargo install mdbook   # one-time
mdbook serve docs      # live reload at http://localhost:3000
mdbook build docs      # static build → target/book/
```

The book covers the architecture, query pipeline, and a per-crate reference.

## Layout

```
repl/                   REPL binary (produces the `helios` executable)
expr/, sql-parser/, optimizer/, physical-plan/, execution/, storage/, row/
                        Workspace crates
docs/                   mdBook documentation
```

Built and tested on Rust stable. RocksDB requires a working C++ toolchain (GCC 13+ or Clang 16+).
