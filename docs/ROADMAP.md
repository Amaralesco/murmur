# Roadmap

**Current position (2026-09-18).** `ingest` is built and running: Jetstream
over WebSocket with the required subprotocol, reconnect with capped
exponential backoff, a read timeout derived from the measured 30s keepalive,
hourly archive rotation, and zstd compression that is verified before the
original is deleted. Ten decisions recorded. Nothing downstream of the disk
exists yet — no broker, no store, no worker.

Phases, not weeks. The original plan was dated and the dates stopped being
true; what remains is the order, which still holds.

Three tracks run in parallel. Reading PostHog's implementation of a component
is the research for building ours, so the hours overlap.

- **Track 1 — Build** (~60%): the pipeline itself.
- **Track 2 — Contribute** (~25%): PRs into `PostHog/posthog` — practice on a
  large real-world Rust codebase that someone else maintains.
- **Track 3 — Read** (~15%): their implementation as a design reference, plus
  one unassisted 25-minute read per cycle.

---

## Phase 1 — Ingest — **done**

Jetstream consumer in Rust: reconnect, backoff, stall detection, archive to
disk, hourly rotation, verified zstd compression.

Still open from this phase: cursor persistence (the hand-written component,
untouched), the `serde_json` unwrap on the hot path, graceful shutdown, and a
retention cap that the docs call for and the code does not implement.

## Phase 2 — One event to a query — **current**

See `docs/tasks/pipeline-thin-slice.md`, whose first slice has its own
document, `docs/tasks/infra-kafka-clickhouse.md`.

A broker and ClickHouse running locally, events produced from Rust, consumed
and batched into ClickHouse, and one query that says something true about the
last hour. Single partition is fine.

**Done when:** an event you can name goes into the socket and comes back out
of a SQL query.

## Phase 3 — Instrument it, then leave it running

Prometheus scraping every service, Grafana with events/sec in, events/sec out,
consumer lag and p99. Telemetry (`docs/tasks/tracing-logging.md`) lands here at
the latest, since a run you cannot diagnose the next morning is not a run.

**Done when:** it has survived 24 hours unattended and the dashboard proves it.

## Phase 4 — Find the wall

- Replay harness: read the corpus, replay at a configurable multiplier.
- Push to 50x. **Predict what breaks first before measuring**, then check
  whether you were right — that gap is the lesson.
- Profile the actual bottleneck. One real optimisation, before and after
  numbers in `docs/BENCHMARKS.md`.
- Partition properly. Choose a key, justify it in `DECISIONS.md`, then observe
  the skew.
- Hot-key overflow: detect high-volume keys, reroute to
  `murmur.events.overflow`, relax ordering, skip enrichment, drop nothing.

**Done when:** you can state a sustained events/sec figure with p99 and the
conditions it was measured under.

## Phase 5 — Identity and failure

- Identity resolution: DID as the stable ID, handle as a mutable alias.
  PostgreSQL lookup with a Redis cache in the hot path, and the merge case
  where two identities turn out to be one actor.
- Measure what a stateful lookup costs the hot path. That is the interesting
  number.
- Multi-tenancy: tenant filters, per-tenant quotas, and a deliberate noisy
  neighbour test.
- Graceful shutdown and health, modelled on their `common/lifecycle`.
- Two injected failures with postmortems: kill the broker mid-write; stall
  ClickHouse. Verify no double-counting on recovery.

**Done when:** two postmortems exist and the recovery path is proven rather
than assumed.

## Later

Move the deployment to a real Linux box. Re-run the benchmark suite there —
those are the numbers worth publishing, since Docker on macOS distorts disk
I/O. Then the product layer: trend and anomaly surfacing, and a tenant-facing
view.

---

## Rituals

**Architecture defence.** Once a cycle, the assistant examines me on an
open-ended design problem I have not seen — 45 minutes, no notes, no lookups.
Then written feedback on where the reasoning was sound and where it was
hand-waving. Kept because defending a design out loud is how you find out
whether you understand it.

**Timed build.** Periodically: eight hours, a problem I did not choose, an
empty directory, AI on and used hard. Ship something that runs and is
instrumented, then write up what was cut and why. What is being practised is
scoping in the first thirty minutes.
