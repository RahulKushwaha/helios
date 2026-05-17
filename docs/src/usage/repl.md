# REPL Guide

The `query_opt` binary starts an interactive SQL REPL backed by RocksDB.

## Starting the REPL

```bash
cargo run                    # uses ./query_opt_data
cargo run -- /path/to/db     # custom data directory
```

Data persists across sessions in the specified directory.

## Dot Commands

| Command | Description |
|---|---|
| `.tables` | List all tables |
| `.schema <table>` | Show column names and types for a table |
| `.quit` / `.exit` | Exit the REPL |

## SQL Support

### DDL

```sql
CREATE TABLE orders (id INT, customer STRING, amount INT);
CREATE INDEX ON orders (customer);
```

### DML

```sql
INSERT INTO orders VALUES (1, 'alice', 100), (2, 'bob', 250);
```

### Queries

```sql
SELECT customer, amount FROM orders WHERE amount > 150;
SELECT customer, SUM(amount) FROM orders GROUP BY customer;
SELECT o.customer, o.amount FROM orders o JOIN users u ON o.customer = u.name;
SELECT customer FROM orders ORDER BY amount DESC LIMIT 10 OFFSET 20;
```

`LIMIT` and `OFFSET` are streaming: the engine stops pulling from the input as soon as `fetch` rows have been emitted.

### EXPLAIN

Prefix any SELECT with `EXPLAIN` to print the logical and physical plan trees without executing the query:

```sql
EXPLAIN SELECT customer FROM orders WHERE amount > 100;
```

Output:

```
Logical Plan:
Projection: Column("customer")
  Filter: BinaryExpr (left: Column("amount"), op: >, right: Literal("100"))
    Scan: orders

Physical Plan:
Projection: Column("customer")
  Filter: BinaryExpr (left: Column("amount"), op: >, right: Literal("100"))
    TableScan: orders
```

## Index Acceleration

When a secondary index exists on the filtered column, the engine uses an index scan instead of a full table scan. Create indexes on frequently filtered columns for better performance:

```sql
CREATE INDEX ON orders (amount);
SELECT * FROM orders WHERE amount = 100;   -- uses index scan
```
