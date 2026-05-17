# physical-plan

Defines the physical execution plan and the planner that converts logical plans into physical ones.

## `PhysicalPlan`

The concrete operations an execution engine must interpret:

```rust
enum PhysicalPlan {
    TableScan { table_name, schema },
    Filter { predicate, input },
    Projection { exprs, input },
    NestedLoopJoin { left, right, on, join_type },
    Sort { exprs, input },
    HashAggregate { group_by, aggr_exprs, input },
    SortAggregate { group_by, aggr_exprs, input },
    ScalarAggregate { aggr_exprs, input },
    Limit { skip, fetch, input },
}
```

Unlike `LogicalPlan`, `PhysicalPlan` specifies *how* to execute. For example, `Join` becomes `NestedLoopJoin` (a future `HashJoin` variant could be added). The single `LogicalPlan::Aggregate` splits into three physical variants depending on input properties:

| Variant | Memory | Output order | Pipeline | Best when |
|---|---|---|---|---|
| `ScalarAggregate` | O(1) | n/a (1 row) | blocking | no GROUP BY |
| `SortAggregate` | O(1) extra | preserves input order | streaming | input pre-sorted on group keys |
| `HashAggregate` | O(group count) | unordered | blocking | general fallback |

## `PhysicalPlanner`

Recursively maps each `LogicalPlan` variant to its physical counterpart:

| Logical | Physical |
|---|---|
| `Scan` | `TableScan` |
| `Filter` | `Filter` |
| `Projection` | `Projection` |
| `Join` | `NestedLoopJoin` |
| `Sort` | `Sort` |
| `Aggregate` | `HashAggregate` (default; strategy selection between Hash/Sort/Scalar is TODO) |
| `Limit` | `Limit` |

The planner currently always picks `HashAggregate` for any `LogicalPlan::Aggregate`. Strategy selection (group_by empty → `ScalarAggregate`; child is `Sort` matching group keys → `SortAggregate`; else `HashAggregate`) is wired through the type system but not yet implemented. `main.rs` performs the same 1:1 mapping inline as a fallback.
