---
title: Roadmap
description: Planned work and development milestones for ByteORM
order: 6
pager: false
---

# Roadmap

<PageProgress pages={pages()} />

## 0.0

- [x] Initial .bo schema language and parser
- [x] PostgreSQL and Rust code generation
- [x] Generated typed database client

## 0.1

- [x] Typed create, read, update and delete API
- [x] Typed query builders and filtering
- [x] Nullable fields with ? syntax
- [x] Enums, defaults, indexes, unique constraints and foreign keys
- [x] JSONB support and typed deserialization
- [x] Computed fields and aggregate queries
- [x] Ordering, limit and offset
- [x] PostgreSQL connection pooling and TLS
- [x] Transactions with commit and rollback
- [x] Raw SQL queries and execution
- [x] Batch insert and upsert operations
- [x] Schema push, reset and repair workflows
- [x] Generated client backed by ByteORM derive macros
- [x] Formatter, self-update and shell completions
- [x] IntelliJ schema highlighting and completion

## 0.2

- [x] Shared query runtime across generated models
- [x] Shared mutation runtime for create, update, delete and upsert
- [x] Inline storage for common query parameters
- [x] Index-based mutation column addressing
- [x] Shared JSONB accessor implementation
- [x] Safer update and delete behavior
- [x] Only rewrite generated files when content changes
- [x] CI for formatting, Clippy, tests and generated clients

## 0.3

- [x] Benchmark suite against Diesel, SeaORM, SQLx and Toasty
- [x] Database warmup and rotating benchmark order
- [x] Prepared statement caching
- [x] Transactions pinned to pooled connections
- [x] Automatic regeneration after generator version changes
- [x] Query and mutation runtime performance optimizations

## Todo

### Schema and types

- [ ] Redesigned concise schema syntax
- [ ] primary, unique and index field keywords
- [ ] Remove NotNull and Nullable in favor of ?
- [ ] Custom Rust field types with as
- [ ] Custom types defined directly in the schema
- [ ] Schema-defined typed JSONB values
- [ ] Simplified default expressions such as @default(now)
- [ ] Consistent PostgreSQL identifier quoting and validation
- [ ] UUID support
- [ ] Numeric and decimal support
- [ ] Byte and bytea support
- [ ] PostgreSQL array types
- [ ] Time and additional timestamp types
- [ ] PostgreSQL network types
- [ ] PostgreSQL identity and improved auto increment support

### Queries

- [ ] Composable AND, OR and NOT filters
- [ ] neq, not_in, between and additional comparison filters
- [ ] contains, starts_with, ends_with, like and ilike filters
- [ ] Typed field selection
- [ ] Generated projected and partial result types
- [ ] Typed query expressions and aliased columns
- [ ] Distinct and PostgreSQL distinct on queries
- [ ] Group by queries
- [ ] Having filters for aggregate queries
- [ ] Typed subqueries and IN subquery filters
- [ ] Cursor and keyset pagination
- [ ] Streaming query results without collecting into Vec
- [ ] Generated typed inputs for bulk operations

### Relations

- [ ] Relations represented directly in the schema
- [ ] One-to-one relations
- [ ] One-to-many relations
- [ ] Many-to-many relations
- [ ] Generated reverse relation accessors
- [ ] Typed nested relation includes
- [ ] Relation filters and relation counts
- [ ] Batched relation loading
- [ ] Typed inner, left and custom joins
- [ ] Transactional nested creates, updates and deletes

### PostgreSQL and schema management

- [ ] Complete constraint and index schema diffing
- [ ] Safer destructive schema changes during push
- [ ] Explicit table and column name mapping
- [ ] Safe table and column renames
- [ ] Database schema drift detection
- [ ] Generate ByteORM schemas from existing PostgreSQL databases
- [ ] Database seeding workflow

### Runtime

- [ ] Configurable connection pool limits and timeouts
- [ ] Structured ByteORM error types and database constraint errors
- [ ] Transaction savepoints and nested transactions
- [ ] Configurable transaction isolation levels
- [ ] Read-only and read-write transaction modes
- [ ] Structured tracing and query observability
- [ ] Connection pool health and lifecycle controls

### Tooling and DX

- [ ] Improved schema diagnostics and formatter support
- [ ] More editor and ecosystem integrations
- [ ] Complete guides, API documentation and examples

### Stability

- [ ] Stable ByteORM schema language
- [ ] Stable generated client API
- [ ] Stable query, relation and mutation APIs
- [ ] Defined backwards compatibility guarantees
- [ ] Official crates.io distribution
- [ ] Production readiness and 1.0 release

## Considered

- [ ] Additional database backends where they make sense
- [ ] Optional versioned migration workflow alongside byteorm push
- [ ] Mock database support for application tests
- [ ] PostgreSQL vector type support
