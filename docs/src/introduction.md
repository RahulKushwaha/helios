# query_opt

A from scratch query optimizer and SQL engine written in Rust.

`query_opt` implements the core components of a relational database query processor: parsing, planning, optimization, physical planning, execution, and persistent storage. It ships as an interactive REPL that accepts SQL statements and executes them against a RocksDB backend.

## Goals

- Explore query optimization techniques (rule based and cost based) in a self contained codebase.
- Provide a working end to end pipeline from SQL text to query results.
- Serve as a learning resource for database internals.

## Project Status

The project is under active development. Many optimizer rules and the distributed execution layer contain `todo!()` stubs marking planned work. The core path (parse → optimize → execute → return results) is functional for single node queries.

## Quick Start

```bash
cargo build
cargo run          # opens the REPL with default data directory ./query_opt_data
cargo run -- /tmp/mydb   # custom data directory
```

Once inside the REPL, try:

```sql
CREATE TABLE users (id INT, name STRING, score INT);
INSERT INTO users VALUES (1, 'alice', 90), (2, 'bob', 75);
SELECT name, score FROM users WHERE score > 80;
EXPLAIN SELECT name FROM users WHERE score > 80;
```
