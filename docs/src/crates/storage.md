# storage

RocksDB backed persistent storage layer.

## `RocksStorage`

The main storage struct. Opens a RocksDB database at a given path and manages tables as column families.

### Table Operations

- `create_table(name, schema)`: creates a column family and persists schema metadata in the `_meta` column family.
- `insert_row(table, row)`: assigns an auto incrementing row ID, serializes the row with `bincode`, and writes it. Also updates any secondary indexes.
- `scan_table(table)`: full table scan, iterating all rows in the column family.
- `get_schema(table)`: returns the `Schema` from the metadata cache.
- `list_tables()`: returns all registered table names.

### Secondary Indexes

- `create_index(table, column)`: creates a dedicated column family (`idx_{table}_{column}`) and backfills from existing rows.
- `has_index(table, column)`: checks if an index exists.
- `index_scan(table, column, op, value)`: scans the index column family for keys matching the condition, then point looks up the matching rows.

Index keys are encoded for correct byte ordering: a type prefix byte, the value in sort preserving binary form (sign flipped integers, IEEE 754 ordered floats, null terminated strings), and the row ID suffix.

### Range Scans

`scan_range(table, start_key, end_key)` reads rows within a key range. Used by the distributed layer for shard local reads. Currently a `todo!()` stub.

## `RocksEngine`

Implements `ExecutionEngine` for any storage that satisfies `Catalog + StorageRead`. Internally it:

1. Tries an index-acceleration shortcut: if the plan is `Filter(predicate, TableScan)` and the predicate constrains an indexed column, it routes through `index_scan` + materializer instead of a full table scan.
2. Otherwise builds a `RowStream` tree via `execution::stream::build_stream` and drains it. The streaming path is shared with any other `ExecutionEngine` implementation — `RocksEngine` only contributes the `DataSource` adapter (`StorageDataSource`) that resolves `TableScan` against RocksDB.

Per-operator behavior (Filter, Projection, NestedLoopJoin, Sort, HashAggregate) lives in the `execution` crate, not here. New streaming operators (`Limit`, `ScalarAggregate`, `SortAggregate`) automatically work for `RocksEngine` because they're wired into `build_stream`.

The crate uses `rocksdb` 0.23 (newer `librocksdb-sys` sources required for GCC 13+ / Clang 16+).
