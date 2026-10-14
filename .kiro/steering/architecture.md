# Architecture

## Component map

```
Bluesky Jetstream (WebSocket, JSON, no auth)
        |
        v
[ingest]      Rust   -- reconnect, cursor mgmt, raw archive to disk,
        |                and producing. Validation, normalisation, tenant
        |                routing and hot-key detection are modules here,
        |                added when needed - not a separate service.
        |                See DECISIONS.md 2026-09-28.
        |
        +--> Kafka: murmur.events           (normal path, ordered per key)
        +--> Kafka: murmur.events.overflow  (hot keys, ordering relaxed)
        +--> Kafka: murmur.events.historical(backfill after disconnect)
                |
                v
[ingestion-consumer]  Rust -- consumes murmur.events. Slice 5 reads and
                |            prints; slice 6 batches into ClickHouse.
                v
        ClickHouse (events, actors)
                |
                v
[api]         Rust   -- tenant queries, trend/anomaly surfacing
```

Process boundaries sit where a queue already separates them. A Kafka topic is a
transport, a buffer and a retry boundary acquired for free; inventing one
between a socket reader and a producer is not. This is why the consumer is a
separate crate and the producer is not — `DECISIONS.md` 2026-09-28.

**Naming.** `ingestion-consumer`, not `worker`. A worker processes discrete jobs
off a queue; this reads a stream and never finishes, which the ecosystem calls a
consumer, a processor or — for "topic in, store out" — a sink. PostHog splits the
same way: `ingestion-consumer` and `cohort-stream-processor` read streams,
`batch-import-worker` and `embedding-worker` run jobs.

**Where transformation belongs is three questions, not one.** An earlier version
of this diagram assigned "transforms, identity resolution, enrichment" to one
component. They do not share an answer:

| | Belongs | Why |
|---|---|---|
| **Enrichment against reference data** — handle → DID, tenant metadata | ClickHouse, as a dictionary or a join | A dictionary is an external lookup table ClickHouse holds in memory and refreshes. PostHog uses them, with `dictGet`, for exchange rates and team selection. Doing the lookup per event in the pipeline instead costs a round trip on the hot path and freezes the result at insert time |
| **Per-row reshaping on insert** | ClickHouse, as a materialized view | No state, no lookups, and the database already has the row |
| **Identity resolution** — deciding two identifiers are one actor | a stateful component, not the database | It needs mutable state and decisions across events. ClickHouse has no transactions and performs updates as asynchronous background rewrites. PostHog spends **ten crates** on this, including leader election and a Stateright model-checked implementation |

For murmur, the deliberate parallel is that handles are mutable while DIDs are
stable. Resolving one to the other is reference data. Merging two identities
into one actor is not, and has no home yet — phase 5 owns it.

**Open, and not decided by this diagram:** the earlier version routed the
consumer's output through a second topic, `murmur.clickhouse`, before the
database. That hop has no entry in `DECISIONS.md` and nothing needs it yet, so
the consumer writes ClickHouse directly. It has the same shape as the
`ingest`/`capture` split that was dissolved on 2026-09-28 — invented up front,
never justified — and deserves the same examination before anything is built on
it. A second topic earns its place when something other than ClickHouse wants
the transformed events, or when transformation becomes expensive enough to
want its own process.

## Deliberate parallels to PostHog

Each is a real problem in this domain, not a contrived exercise.

| Their problem | Our equivalent |
|---|---|
| Rust capture service, fast path to Kafka | `ingest` — same responsibility, same shape, and like theirs a single process owning intake and producing |
| `events_plugin_ingestion_overflow` for hot distinct IDs | viral post URIs create genuine hot partitions |
| Person merge via `$identify` / `$anon_distinct_id` | handles are mutable, DIDs are stable → real aliasing |
| `events_plugin_ingestion_historical` | reconnect backfill after socket drop |
| Node.js CDP worker doing person processing | `worker`, same job, in Rust |
| Multi-tenant isolation and noisy neighbours | tenant filters; a tenant subscribing to `*` is the abuse case |
| Schema evolution across 450k orgs | AT Protocol lexicons evolve; third parties invent record types |

## Language boundaries (intentional)

- **Rust everywhere.** `ingest`, `capture` and the consumer that writes to
  ClickHouse are all Rust. The project exists to learn Rust, so a second
  language subtracts from the point rather than adding to it.
- **Node/TypeScript is deferred, not forbidden.** It was originally chosen for
  `worker` and `api` in order to practise both languages — see `DECISIONS.md` 2026-09-18. If an
  API or front-end layer later warrants TypeScript, it can be its own
  exercise, taken on when Kafka and ClickHouse are no longer new.

Rust is also where allocation, copy avoidance and async runtime behaviour are
visible, which was the original reason the hot path was Rust.

## Infrastructure stance

- Self-hosted Kafka, ClickHouse, PostgreSQL, Redis. **Never** substitute a
  managed service — feeling rebalances, partition skew, lag, and merge pressure
  is the learning objective.
- Local Docker Compose for development (M4 Pro, 12 cores, 48 GB).
- Published benchmarks must come from a Linux box, not Docker on macOS — the
  macOS VM layer distorts disk I/O and makes numbers non-comparable.
- Target metric is **throughput per core per euro**, not absolute volume.

## Load generation

Record the live firehose to disk, then replay at 50-100x. This gives synthetic
scale with *real* distributions: genuine Zipfian hot keys, real malformed
payloads, real burst shapes. A uniform load generator produces none of these.

The replay corpus is also the regression suite. Same data, every week, watch
the number move. Cap retention — disk is the binding local constraint.
