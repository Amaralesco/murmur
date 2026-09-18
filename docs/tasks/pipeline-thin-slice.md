# Story — one event from the firehose to a ClickHouse query

**Status: TODO** (opened 2026-09-18)

Source of truth: `docs/WEEK-1.md` (Wednesday and Thursday),
`.kiro/steering/architecture.md`.

---

## The story

Today `ingest` archives the firehose to disk and stops there. Kafka,
ClickHouse and the Node worker — most of the architecture — have never run.
This story ends when **one event you can name goes into the socket and comes
back out of a SQL query**.

Not a feature set. One event, provably, through every stage.

**Why now.** `ingest` is by a distance the most developed part of the project:
reconnect, backoff, stall detection, rotation, verified compression, ten
recorded decisions. The remaining learning is in the parts that have never
run — a message broker, a columnar store, and the language boundary.

## Ground rule for this story

Each slice ends with **something you can see**, and then you stop. No slice
needs the next one to be worth doing. If a slice starts sprawling, it is two
slices.

---

## Slice 1 — the infrastructure runs

Docker Compose with Kafka and ClickHouse. Nothing else yet.

**Done when:** you create a topic, produce a message and consume it back with
the console tools, and `SELECT 1` answers from `clickhouse-client`. No Rust,
no Node.

## Slice 2 — a table that holds an event

One ClickHouse table, by hand, in SQL.

**Done when:** you `INSERT` one row and `SELECT` it back.

## Slice 3 — Rust produces one message

A hardcoded string to `murmur.events`, using `rdkafka`. Nothing about
Jetstream.

**Done when:** the console consumer prints your string.

## Slice 4 — real events reach the topic

Wire the producer into the live path, so events from the firehose land in
`murmur.events`.

**Done when:** the console consumer shows Jetstream events flowing while
`ingest` runs.

## Slice 5 — the Node worker consumes

A TypeScript consumer that prints what it receives. Your first TypeScript in
this project, so keep it to consuming and printing.

**Done when:** it prints events that Rust produced.

## Slice 6 — the worker writes to ClickHouse

Batch the consumed events and insert them.

**Done when:** `SELECT count()` grows while the worker runs.

## Slice 7 — one true sentence

A query that says something true about the last hour.

**Done when:** you can state the number and defend how it was produced.

---

## Decisions you own

- **Where the producer lives.** `architecture.md` says `capture` validates,
  normalises and produces, with `ingest` owning only the socket and the
  archive. The thin slice could cheat and produce straight from `ingest`.
  Cheating is faster now and costs a move later. Decide deliberately, because
  building `capture` is also the trigger for the `common`-crate review in
  `DECISIONS.md` 2026-09-02, and the point where telemetry comes back.
- **What an event is on the wire.** Raw Jetstream JSON, or normalised into
  your own shape before it reaches Kafka?
- **The table.** Engine, `ORDER BY`, and which fields. `seq`, `did`,
  `collection`, `time` and the record are all candidates; you do not need all
  of them.
- **The Node Kafka client.** PostHog uses `node-rdkafka` (native bindings,
  a build step). `kafkajs` is pure JavaScript and much easier to start with.
  Different trade-off from theirs, and defensible.
- **Batching in the worker.** Size, time, or both — and what happens to a
  partial batch when the process stops.
- **Duplicates.** Delivery is at-least-once and the Jetstream cursor is
  inclusive, so the same event can arrive twice. Deduplicate, or accept and
  record it?

## Pointers

| What | Where |
|---|---|
| Rust Kafka client | `rdkafka 0.37.0`, `posthog/rust/Cargo.toml:194` |
| Node ClickHouse client | `@clickhouse/client ^1.12.0`, `posthog/nodejs/package.json:72` |
| Node Kafka client (theirs) | `node-rdkafka ^3.6.1`, same file, `:130` |
| ClickHouse DDL to read | `posthog/posthog/clickhouse/migrations/sql/`, `posthog/bin/clickhouse-logs.sql` |
| Their Node ingestion path | `posthog/nodejs/src/ingestion/ingestion-consumer.ts` |

## Out of scope

PostgreSQL, Redis, identity resolution, more than one partition, overflow
topics, retention, Prometheus and Grafana. All of those have their own week.

Telemetry (`docs/tasks/tracing-logging.md`) is deferred until `capture`
exists — see that document.
