# Story — one event from the firehose to a ClickHouse query

**Status: CURRENT** (opened 2026-09-18)

Source of truth: `docs/ROADMAP.md` (phase 2), `.kiro/steering/architecture.md`.

---

## The story

Today `ingest` archives the firehose to disk and stops there. Kafka and
ClickHouse — most of the architecture — have never run.
This story ends when **one event you can name goes into the socket and comes
back out of a SQL query**.

Not a feature set. One event, provably, through every stage.

**Why now.** `ingest` is by a distance the most developed part of the project:
reconnect, backoff, stall detection, rotation, verified compression, ten
recorded decisions. The remaining learning is in the parts that have never
run — a message broker and a columnar store.

## Ground rule for this story

Each slice ends with **something you can see**, and then you stop. No slice
needs the next one to be worth doing. If a slice starts sprawling, it is two
slices.

---

## Slice 1 — the infrastructure runs — **done 2026-09-22**

**Has its own document: `docs/tasks/done/infra-kafka-clickhouse.md`.**

The largest slice in this story by some distance, and the one whose failure
mode is least obvious. Read the advertised-listener section there before
writing any YAML — the symptom is "works inside the container, my host client
connects and then dies", and it looks like a bug in your code.

Docker Compose with a Kafka-compatible broker and ClickHouse. Nothing else
yet.

**Done when:** both are reachable from your host rather than only from inside
Docker, and both survive a `down` / `up`. No Rust, no Node.

## Slice 2 — a table that holds an event — **done 2026-09-23**

One ClickHouse table, by hand, in SQL. `murmur.events`, defined in
`clickhouse/schema.sql` with the reasoning in `clickhouse/schema-notes.md`.

**Done when:** you `INSERT` one row and `SELECT` it back.

## Slice 3 — Rust produces one message — **current**

A hardcoded string to `murmur.events`, using `rdkafka`. Nothing about
Jetstream.

**Done when:** the console consumer prints your string.

### Then automate it — the first test in the repo

Slice 1 proved the broker by hand: create a topic, produce, consume it back.
That proof is worth keeping, and now is when it is worth automating, because
one test can cover the broker and the producer in a single pass rather than
two half-tests written months apart.

It is an **integration** test, not a unit test — it needs a real broker and
real I/O, so it is seconds per run, not microseconds, and belongs in the
crate's `tests/` directory rather than beside the code.

Two ways to get a broker, and this is the decision:

- **`testcontainers`** starts its own Kafka from inside the test and tears it
  down after. Clean state every run, nothing to have running first. PostHog
  takes this route — `posthog/rust/Cargo.toml:276`.
- **Point at the running Compose stack.** Faster and simpler, but the test
  fails when you forgot to bring the stack up, and tests share state.

**Done when:** `cargo test` produces a message and reads it back, with no
manual step before it.

## Slice 4 — real events reach the topic

Wire the producer into the live path, so events from the firehose land in
`murmur.events`.

**Done when:** the console consumer shows Jetstream events flowing while
`ingest` runs.

## Slice 5 — a consumer reads the topic

A second Rust binary that consumes `murmur.events` and prints what it
receives. Consumer groups and offsets are the new material here; keep it to
consuming and printing.

**Done when:** it prints events that the producer wrote, and restarting it
resumes from where it stopped rather than from the beginning.

## Slice 6 — the consumer writes to ClickHouse

Batch the consumed events and insert them.

**Done when:** `SELECT count()` grows while the consumer runs.

## Slice 7 — one true sentence

A query that says something true about the last hour.

**Done when:** you can state the number and defend how it was produced.

---

## Decisions you own

**DECIDED — where the producer lives.** Inside `ingest`. `capture` is not built
as a separate service; its responsibilities become modules there when needed.
`DECISIONS.md` 2026-09-28, which also moves the two triggers that hung off
`capture` — the `murmur-common` review and telemetry — onto the slice 5
consumer crate.

**DECIDED — where the consumer lives.** Its own crate in the workspace. That is
the process boundary in this story, because a Kafka topic is already a transport
between the two sides.

**DECIDED — the table.** `ReplacingMergeTree`, sorting key
`(jetstream_event_at, uri)`, twelve columns. Slice 2, and the reasoning is in
`clickhouse/schema-notes.md`.

**DECIDED — duplicates.** Deduplicated in ClickHouse rather than in the
pipeline. The sorting key collapses a redelivered event while keeping two
genuine events about one record, which was measured rather than assumed.

Still open:

- **What an event is on the wire.** Raw Jetstream JSON, or normalised into
  your own shape before it reaches Kafka?
- **Offset commits.** Commit before writing to ClickHouse, or after? Before
  risks losing a batch on a crash; after risks writing it twice. Delivery is
  already at-least-once, so this decides which failure you prefer.
- **Batching in the consumer.** Size, time, or both — and what happens to a
  partial batch when the process stops.

## Pointers

| What | Where |
|---|---|
| Rust Kafka client | `rdkafka 0.37.0`, `posthog/rust/Cargo.toml:194` |
| Rust ClickHouse client | `clickhouse 0.13.2`, `posthog/rust/Cargo.toml:263` |
| Consumer loop to model on | `posthog/rust/kafka-deduplicator/src/kafka/batch_consumer.rs:356-390` |
| ClickHouse DDL to read | `posthog/posthog/clickhouse/migrations/sql/`, `posthog/bin/clickhouse-logs.sql` |
| Their Rust ingestion consumer | `posthog/rust/ingestion-consumer/` |

## Out of scope

PostgreSQL, Redis, identity resolution, more than one partition, overflow
topics, retention, Prometheus and Grafana. All of those have their own week.

Telemetry (`docs/tasks/medium/tracing-logging.md`) is deferred until the slice 5
consumer crate exists — see that document.
