# optimizer

The rule based and cost based query optimizer. Contains 30+ rewrite rules and a cost model.

## Core Abstractions

### `OptimizerRule` trait

```rust
pub trait OptimizerRule {
    fn name(&self) -> &str;
    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan, OptimizerError>;
}
```

Each rule receives a `LogicalPlan`, returns a (possibly rewritten) plan, and is expected to be idempotent.

### `Optimizer`

Holds a `Vec<Arc<dyn OptimizerRule>>` and runs them in a fixed point loop up to `max_passes` (default 16). The loop terminates early when no rule changes the plan.

## Rules

| Rule | Description |
|---|---|
| `PushDownFilter` | Move filter predicates below projections and into join children |
| `PushDownProjection` | Prune columns as early as possible |
| `ConstantFolding` | Evaluate constant expressions at plan time |
| `ConstantPropagation` | Replace column references with known constant values |
| `PredicateSimplification` | Simplify boolean expressions |
| `FilterMerge` | Combine adjacent filter nodes |
| `ProjectionElimination` | Remove identity projections |
| `EliminateCrossJoin` | Convert cross join + filter into inner join |
| `SortElimination` | Remove redundant sorts |
| `TransitivePredicate` | Derive new predicates from equality chains |
| `PredicateDecomposition` | Split compound predicates into conjuncts |
| `JoinTypeConversion` | Convert outer joins to inner joins when possible |
| `JoinElimination` | Remove unnecessary joins |
| `JoinCommutation` | Swap join sides for better performance |
| `JoinReorder` | Reorder multi way joins using cost estimates |
| `AggregatePushdown` | Push aggregates below joins |
| `AggregateMerge` | Combine adjacent aggregates |
| `AggregateElimination` | Remove unnecessary aggregates |
| `DistinctToAggregate` | Rewrite DISTINCT as GROUP BY |
| `SingleDistinctToGroupBy` | Optimize single DISTINCT aggregate |
| `UnionMerge` | Flatten nested unions |
| `LimitPushdown` | Push LIMIT into children |
| `CommonSubexprElimination` | Factor out repeated subexpressions |
| `TypeCoercion` | Insert implicit casts |
| `NullPropagation` | Simplify expressions involving NULL |
| `PropagateEmptyRelation` | Short circuit plans that produce no rows |
| `UnwrapCastInComparison` | Remove unnecessary casts in comparisons |
| `Decorrelation` | Decorrelate correlated subqueries |
| `SubqueryToJoin` | Rewrite subqueries as joins |
| `ScalarSubqueryFlattening` | Flatten scalar subqueries |

> Most rules currently contain `todo!()` stubs and are ready for implementation.

## Cost Model

The `CostModel` trait and `SimpleCostModel` implementation estimate plan cost using row counts and fixed selectivity factors:

| Node | Cost Formula |
|---|---|
| Scan | `row_count` (default 1000) |
| Filter | `child_cost × 0.1` |
| Projection | `child_cost` |
| Join | `left_rows × right_rows` |
| Sort | `rows × log2(rows)` |
| Aggregate | `rows` |

`pick_best_plan` selects the cheapest plan from a set of candidates.
