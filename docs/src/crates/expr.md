# expr

The foundational crate that every other crate depends on. It defines the shared type system, expression tree, logical plan, and statistics structures.

## Modules

### `types`

Runtime values and column data types.

- **`FieldValue`**: `Int(i64)`, `Float(f64)`, `Str(String)`, `Bool(bool)`, `Null`. Implements `Hash` and `Eq` (used as hash-table keys in aggregation), and serializes with `serde`.
- **`DataType`**: `Int`, `Float`, `Str`, `Bool`. Used in schema definitions.

### `schema`

- **`Field`**: a column name paired with a `DataType`.
- **`Schema`**: an ordered list of `Field`s with a `HashMap` lookup for O(1) name resolution via `field_by_name`.

### `expr`

The expression tree used in filters, projections, join conditions, and aggregates.

```rust
enum Expr {
    Column(String),
    Literal(FieldValue),
    BinaryExpr { left: Box<Expr>, op: Operator, right: Box<Expr> },
    AggregateFunction { fun: AggFunc, args: Vec<Expr> },
}
```

Operators cover comparison (`Eq`, `Lt`, …), arithmetic (`Plus`, `Minus`, …), and logical (`And`, `Or`).

Aggregate functions: `Count`, `Sum`, `Min`, `Max`, `Avg`.

### `logical_plan`

- **`LogicalPlan`**: the relational algebra tree with variants `Scan`, `Filter`, `Projection`, `Join`, `Sort`, `Aggregate`, `Limit { skip, fetch, input }`.
- **`JoinType`**: `Inner`, `Left`, `Right`, `Full`.
- **`LogicalPlanBuilder`**: chainable builder API for constructing plans in tests and the SQL planner.
- **`display`**: pretty printing for plan trees.

Each `LogicalPlan` node can derive its output `Schema` via the `.schema()` method.

### `statistics`

- **`ColumnStatistics`**: optional `distinct_count`, `min_value`, `max_value`.
- **`Statistics`**: optional `row_count` plus per column statistics. Used by the cost model.
