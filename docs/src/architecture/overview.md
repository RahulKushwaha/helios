# Architecture Overview

`query_opt` is organized as a Cargo workspace with seven crates, each responsible for one layer of the query processing stack.

```
┌─────────────┐
│  sql-parser  │   SQL text → LogicalPlan
├─────────────┤
│  optimizer   │   LogicalPlan → optimized LogicalPlan
├──────────────┤
│ physical-plan│   LogicalPlan → PhysicalPlan
├──────────────┤
│  execution   │   PhysicalPlan → result rows
├──────────────┤
│   storage    │   RocksDB backed table/index I/O
├──────────────┤
│ distributed  │   Partitioning, fragments, exchanges
└──────────────┘
        ▲
        │
┌──────────────┐
│     expr     │   Shared types: FieldValue, DataType, Schema,
│              │   Expr, LogicalPlan, Statistics
└──────────────┘
```

The `expr` crate sits at the bottom of the dependency graph. Every other crate depends on it for the common type definitions.

## Workspace Layout

| Crate | Path | Role |
|---|---|---|
| `expr` | `expr/` | Core types, expressions, logical plan, statistics |
| `sql-parser` | `sql-parser/` | SQL parsing via `sqlparser` crate, conversion to `LogicalPlan` |
| `optimizer` | `optimizer/` | 30+ rewrite rules and a cost model |
| `physical-plan` | `physical-plan/` | `PhysicalPlan` enum and logical to physical conversion |
| `execution` | `execution/` | `ExecutionEngine` / `DataSource` / `RowStream` / `Aggregator` traits, batched pull-based streams, expression evaluator |
| `storage` | `storage/` | RocksDB storage layer with secondary indexes |
| `distributed` | `distributed/` | Partition map, plan fragments, exchanges, coordinator |

The top level binary (`src/main.rs`) wires everything together into an interactive REPL.
