# sql-parser

Converts SQL text into `LogicalPlan` trees (or DDL/DML commands).

## Dependencies

Uses the [`sqlparser`](https://crates.io/crates/sqlparser) crate with `GenericDialect` for tokenizing and parsing.

## Key Types

- **`SqlPlanner`**: holds a catalog (`HashMap<String, Schema>`) and exposes `plan_sql(&str) -> Result<SqlStatement, PlanError>`.
- **`SqlStatement`**: the output of planning:
  - `CreateTable { name, schema }`
  - `CreateIndex { table, column }`
  - `Insert { table, rows }`
  - `Query(LogicalPlan)`

## How It Works

1. `Parser::parse_sql` produces an AST.
2. `plan_statement` dispatches on the AST node type.
3. For `SELECT` queries, `plan_query` walks the AST to build a `LogicalPlan` using `LogicalPlanBuilder`: resolving table references against the catalog, mapping SQL expressions to `Expr`, and layering `Filter`, `Projection`, `Join`, `Sort`, `Aggregate`, and `Limit` nodes.

`LIMIT` and `OFFSET` clauses parse to a `LogicalPlan::Limit { skip, fetch, input }` wrapping the rest of the query. `OFFSET` without `LIMIT` uses `usize::MAX` as the fetch sentinel; `LIMIT` without `OFFSET` uses `skip = 0`.

## Error Types

`PlanError` variants: `Parse`, `Unsupported`, `TableNotFound`, `ColumnNotFound`.
