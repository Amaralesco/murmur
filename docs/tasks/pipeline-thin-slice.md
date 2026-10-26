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

## Slice 3 — Rust produces one message — **done 2026-09-30**

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

## Slice 4 — real events reach the topic — **done 2026-10-02**

Wire the producer into the live path, so events from the firehose land in
`murmur.events`.

**Done when:** the console consumer shows Jetstream events flowing while
`ingest` runs.

## Slice 5 — a consumer reads the topic — **done 2026-10-26**

**Has its own document: `docs/tasks/done/consumer-reads-topic.md`**, including
how both claims were proved.

A second Rust binary that consumes `murmur.events` and prints what it
receives. Consumer groups and offsets are the new material here; keep it to
consuming and printing.

**Done when:** it prints events that the producer wrote, and restarting it
resumes from where it stopped rather than from the beginning.

## Slice 6 — the consumer writes to ClickHouse — **current**

Batch the consumed events and insert them.

**Done when:** `SELECT count()` grows while the consumer runs.

### What is on the wire, decided in slice 4

The message is the **raw Jetstream JSON**, byte-identical to the archive, with
the `at://` URI as the Kafka key. Nothing is promoted into the payload.

So the consumer parses once, into the shared type:

```rust
let parsed: JetstreamMessage = serde_json::from_str(&payload)?;
```

Every ClickHouse column comes out of that: `did`, `collection`, `rkey`, `rev`,
`seq`, `operation`, `cid` and `time` from `parsed.payload`, `text` and
`createdAt` from `parsed.payload.record`, `uri` from `Commit::uri()` or the
message key, and `murmur_ingested_at` from the table's own `DEFAULT now64(3)`.

The alternatives were rejected for reasons worth not relitigating:

- **Promoting fields into the payload** alongside the raw JSON — an "envelope"
  — costs about 40% more bytes to save a parse worth microseconds. Its real
  claim was decoupling the consumer from Jetstream's shape, and that is
  already provided by `murmur_common::jetstream`: the consumer depends on a
  Rust type in this workspace, not on Bluesky's wire format. If AT Protocol
  changes, one struct changes and both binaries follow.
- **Normalising into murmur's own shape**, discarding the original, loses
  everything not modelled — `reply`, `langs`, `embed`, `facets`. The post-shaped
  table deferred in `clickhouse/schema-notes.md` needs `reply`, and the
  upstream replay window is bounded, so that loss is unrecoverable rather than
  inconvenient.

**Carry forward to phase 4.** When hot-key routing needs to pick a topic
without parsing the payload, the promoted fields belong in **Kafka headers**,
not in the value. Headers are readable without deserialising, which is the
whole point, and they can be added without changing the value format — so no
migration. PostHog does exactly this: `CapturedEventHeaders` becomes
`OwnedHeaders` alongside an untouched `data` field.

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

**DECIDED — group id, offset reset, commit strategy, malformed payloads.**
Slice 5; `DECISIONS.md` 2026-10-26. Note what that entry says about commits:
auto-commit on a timer *and* an explicit `commit_message`, with
`enable.auto.offset.store` left on, so an offset is marked committable the
moment `recv()` hands the message over. Fine for printing, wrong for inserting.

Still open:

- **Offset commits, for real this time.** The slice 5 configuration commits
  offsets for messages merely *received*, so a crash between receive and insert
  loses them. Commit before writing to ClickHouse or after? Before risks losing
  a batch; after risks writing it twice. Delivery is already at-least-once, so
  this decides which failure you prefer — and the current setting decided
  "before" by default rather than by choice.
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

Telemetry landed with slice 5, in both binaries —
`docs/tasks/done/tracing-logging.md`.
