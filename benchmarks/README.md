# ByteORM benchmark suite

Measures ByteORM against Diesel, SeaORM, SQLx and Toasty
on one Postgres instance, one schema and one workload, plus a raw
`tokio-postgres` baseline so every ORM number can be read as overhead over the
driver.

Four things are measured:

| dimension | what it answers |
|---|---|
| runtime | latency and throughput per operation: p50/p95/p99, ops/s |
| compile time | cold build, rebuild after a schema change, code generation |
| ergonomics | lines of code the same workload costs in each ORM |
| memory | peak RSS of the process that ran the workload |

## Quick start

```bash
cd benchmarks
cargo run --release --bin bench -- prepare      # generate the ByteORM client, start Postgres
cargo run --release --bin bench -- all          # runtime + compile time + LOC
```

Reports land in `results/report.md` and `results/report.json`, and are also
printed to stdout.

`prepare` starts `postgres:17-alpine` through `docker-compose.yml` on port
`55432`, with `fsync=off` and the data directory in tmpfs - the goal is to
measure ORM overhead, not disk latency. Set `DATABASE_URL` to point the suite
at your own database instead; Docker is then never touched.

## Commands

```bash
bench up                 # start Postgres and (re)create the schema
bench down               # stop it and delete the volume
bench prepare            # byteorm generate + up
bench run [options]      # runtime benchmark only
bench compile [--cold]   # compile-time benchmark only
bench loc                # lines of code only
bench all [options]      # everything
```

Useful `run` options:

```bash
--orm byteorm,sqlx            # subset of ORMs (default: all)
--scenario find_by_pk,count   # subset of scenarios (default: all)
--iterations 2000             # measured iterations per scenario
--warmup 200                  # discarded iterations before measuring
--concurrency 16              # concurrent tasks issuing the workload
--pool-size 20                # connection pool size (matches ByteORM's fixed 20)
--batch-size 100              # rows per insert_many
--isolate false               # run all ORMs in one process (loses per-ORM RSS)
--out results/run-2           # output directory
```

## Scenarios

| key | operation |
|---|---|
| `insert_one` | INSERT one user, returning the generated row |
| `insert_many` | batch INSERT of N posts in one statement |
| `find_by_pk` | SELECT one user by primary key |
| `find_by_unique_email` | SELECT one user by unique email |
| `recent_posts` | SELECT a user's posts, ordered, limited |
| `update_one` | UPDATE one post by primary key |
| `delete_one` | DELETE one post by primary key |
| `count` | SELECT COUNT(*) of a user's posts |
| `sum_views` | SELECT SUM(views) of a user's posts |
| `transaction` | one transaction: insert a user and its posts |

## Fairness rules

These are the decisions that make the numbers comparable; read them before
quoting any result.

* **One schema, applied once.** `sql/schema.sql` is the only DDL. No ORM
  creates its own tables, so none gets a different physical layout, and
  `SERIAL` keys keep id generation on the database side for everyone.
* **Fixtures are seeded through raw `tokio-postgres`**, never through an
  adapter, and are re-seeded before every scenario. Every ORM meets the same
  row counts and the same planner statistics.
* **Idiomatic code only.** Each adapter uses the API its own documentation
  recommends: Diesel's DSL, SeaORM's `ActiveModel`, ByteORM's generated
  builders, Toasty's `create!`/`filter_by_*`, SQLx's `query_as`. An ORM that
  cannot express an operation reports `n/a` rather than dropping to raw SQL -
  that is a result, not a gap to paper over.
* **Same async runtime.** Diesel runs through `diesel-async`, not
  `spawn_blocking`, so no adapter pays for a thread pool the others avoid.
* **Equal pools.** ByteORM's generated client hardcodes a bb8 pool of 20 and
  exposes no knob for it, so `--pool-size` defaults to 20 and every other
  adapter is built with that same size.
* **Warmup is discarded.** Connections, prepared-statement caches and the
  query planner are warm before the first measured iteration.
* **Throughput comes from the wall clock** of the measured phase, not from
  `1/mean`, so `--concurrency` shows up honestly.
* **One process per ORM** (`--isolate`, on by default). Peak RSS is then
  attributable, and Toasty - whose dependency tree cannot share a Cargo
  resolution graph with SQLx's, since both reach a crate that links
  `sqlite3` - runs under the same harness as everything else.

## Reading the results

* `vs best` is the ratio to the fastest ORM in that scenario.
* The raw baseline is *naive* `tokio-postgres`: it sends SQL text and lets the
  driver prepare each statement, with no statement cache. ORMs that keep a
  prepared-statement cache (SQLx, and everything built on it) can therefore
  beat it. That is the intended comparison - it shows what statement caching
  is worth - not a harness bug.
* `generated` in the ergonomics table is code a generator emits into the
  developer's tree. Nobody writes it by hand, but it is shipped, reviewed and
  compiled, so it is reported rather than ignored.
* Percentiles use nearest-rank on the sorted sample vector.

## Layout

```
benchmarks/
  sql/schema.sql           the one schema every ORM maps onto
  docker-compose.yml       throwaway Postgres 17
  crates/bench-core        harness: contract, fixtures, timing, stats, report
  crates/bench-raw         baseline: hand-written SQL over tokio-postgres
  crates/bench-byteorm     ByteORM adapter + schema.bo + generated client
  crates/bench-diesel      Diesel (diesel-async + bb8)
  crates/bench-seaorm      SeaORM 2.0
  crates/bench-sqlx        SQLx 0.9
  crates/bench-toasty      Toasty 0.10 (own workspace, own binary)
  crates/bench-runner      the `bench` CLI
  results/                 report.json + report.md
```

## Adding an ORM

1. Create `crates/bench-<name>` with a `connect(url, pool_size) -> Arc<dyn OrmAdapter>`.
2. Implement `OrmAdapter` (ten operations) the way that ORM's docs would.
3. Register it in `crates/bench-runner/src/registry.rs`, and add entries to
   `compile.rs` and `loc.rs`.

If its dependencies clash with the workspace, give it its own `[workspace]`
and a `main.rs` that calls `bench_core::child::run_and_write`, then register it
as `Kind::External` - that is exactly how Toasty is wired.
