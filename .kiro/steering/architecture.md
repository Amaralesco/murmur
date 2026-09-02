# Architecture

## Component map

```
Bluesky Jetstream (WebSocket, JSON, no auth)
        |
        v
[ingest]      Rust   -- reconnect, cursor mgmt, raw archive to disk
        |
        v
[capture]     Rust   -- validate, normalise, tenant routing, hot-key detection
        |
        +--> Kafka: murmur.events           (normal path, ordered per key)
        +--> Kafka: murmur.events.overflow  (hot keys, ordering relaxed)
        +--> Kafka: murmur.events.historical(backfill after disconnect)
                |
                v
[worker]      Node/TS -- transforms, identity resolution, enrichment
                |         (PostgreSQL lookup + Redis cache in hot path)
                v
        Kafka: murmur.clickhouse
                |
                v
        ClickHouse (events, actors)
                |
                v
[api]         Node/TS -- tenant queries, trend/anomaly surfacing
```

## Deliberate parallels to PostHog

Each is a real problem in this domain, not a contrived exercise.

| Their problem | Our equivalent |
|---|---|
| Rust capture service, fast path to Kafka | `capture` — same responsibility, same shape |
| `events_plugin_ingestion_overflow` for hot distinct IDs | viral post URIs create genuine hot partitions |
| Person merge via `$identify` / `$anon_distinct_id` | handles are mutable, DIDs are stable → real aliasing |
| `events_plugin_ingestion_historical` | reconnect backfill after socket drop |
| Node.js CDP worker doing person processing | `worker`, same language, same job |
| Multi-tenant isolation and noisy neighbours | tenant filters; a tenant subscribing to `*` is the abuse case |
| Schema evolution across 450k orgs | AT Protocol lexicons evolve; third parties invent record types |

## Language boundaries (intentional)

- **Rust** for `ingest` and `capture` only — the hot path. Smallest possible
  surface so it stays finishable, and it is where allocation, copy avoidance,
  and async runtime behaviour actually matter.
- **Node/TypeScript** for `worker` and `api`. Practising both languages is a
  goal of the project, and PostHog's real worker is Node.

Do not suggest rewriting the worker in Rust for performance without a
measurement showing the worker is the bottleneck.

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
