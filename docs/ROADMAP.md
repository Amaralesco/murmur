# Roadmap

**Current position:** Week 0 — skeleton created, nothing built. First
unassisted read of PostHog's `rust/capture` done and graded
(`docs/reading/capture-service.md`). Verdict: strong inference about what the
service is for, weak navigation. Fix was structural, not knowledge — read
`lib.rs` second, always.

Three tracks run in parallel. Reading PostHog's implementation of a component
is the research phase for building ours, so the hours overlap.

- **Track 1 — Build** (~60%): the pipeline itself. Also timed-build rehearsal.
- **Track 2 — Contribute** (~25%): PRs into `PostHog/posthog`, filtered to
  `feature/team-ingestion` — contributing to a large real-world Rust project.
- **Track 3 — Read** (~15%): their implementation as design reference, plus one
  unassisted 25-minute read per week.

Architecture defence is not a percentage. It is a fixed Friday ritual, because
defending an open-ended design out loud is central to engineering and worth
deliberate practice.

---

## Week 1 — Thin slice, end to end

Reading (Mon): get `PostHog/posthog` running locally. Read `rust/capture`
properly, AI on this time — specifically `lib.rs`, `router`, `v0_endpoint`,
`sinks`. Notes to `docs/reading/capture-service.md`.

Build:
- Jetstream WebSocket consumer in Rust, with reconnect and cursor persistence.
- Raw archive to local disk, zstd, **with a retention cap** — disk is the
  binding constraint at 114 GB free.
- Minimal capture: validate, normalise, produce to `murmur.events`.
- Single-partition Kafka is fine this week.
- Node worker: consume, write batches to ClickHouse.
- One working query that returns something true about the last hour.
- Prometheus + Grafana: events/sec in, events/sec out, consumer lag, p99.

Config comes from **environment variables**, not files. PostHog uses
`envconfig` and `Config::init_from_env()` for exactly this reason — it is the
convention for containerised services. Match it.

Hand-written component: **cursor persistence only.** Not the whole consumer.
It forces `Result`, `?`, file I/O, and serde, and if it takes six hours instead
of one it does not sink the week.

Grafana must exist before Thursday night, because the 24-hour clock has to
start then. It is not a Friday task.

Done when: it has survived 24 hours unattended and the dashboard proves it.

Track 2: one issue in an ingestion-area crate triaged — reproduce it and
comment. No PR needed.

---

## Week 2 — Find the wall

Reading (Mon): their Node ingestion path — `nodejs/src/ingestion/`, in
particular `ingestion-consumer.ts` and `pipelines/`. (Earlier versions of this
roadmap pointed at `nodejs/src/worker/ingestion/event-pipeline`, which no longer
exists — they restructured.) Also `rust/capture`'s `global_rate_limiter` and
`ordering` modules — a large share of their capture service is backpressure and
overflow, which is the whole subject of this week.

Build:
- Replay harness: read the corpus, replay at configurable multiplier.
- Push to 50x. Find what breaks first. **Predict it before you measure**, then
  check whether you were right — that gap is the lesson.
- Profile the actual bottleneck. One real optimisation, before/after numbers in
  `docs/BENCHMARKS.md`.
- Partition properly. Choose a key, justify it in DECISIONS, then observe skew.
- Hot-key overflow: detect high-volume keys, reroute to
  `murmur.events.overflow`, relax ordering, skip enrichment, drop nothing,
  still return success.

Done when: you can state a sustained events/sec figure with p99 and the
conditions it was measured under.

Track 2: first PR opened.

---

## Week 3 — Identity and failure

Reading (Mon): their person-processing and merge logic, which now spans two
languages — `nodejs/src/ingestion/common/persons` on the Node side, and the Rust
`personhog-*` crates (`personhog-identity`, `-router`, `-writer`, `-leader`,
`-replica`, `-coordination`) which the Node side calls over protobuf. Note
`personhog-stateright` implies formal model-checking; leader election and
replicas imply a replicated stateful service. Their own docs call this the most
complex step in the pipeline, and the architecture backs that up.

Build:
- Identity resolution: DID as stable ID, handle as mutable alias. PostgreSQL
  lookup with Redis cache in the hot path. Handle the merge case when two
  identities turn out to be the same actor.
- Measure the cost of adding a stateful lookup to the hot path. This is the
  interesting number.
- Multi-tenancy: tenant filters, per-tenant quotas, and a deliberate noisy
  neighbour test (one tenant subscribing to everything).
- Graceful shutdown and health, modelled on their `common/lifecycle`: trap
  signals, prestop check, wait for background components to finish cleanly.
  Without this, "what happens if it dies mid-write" has no answer.
- Two injected failures, each with a postmortem: kill Kafka mid-write; throttle
  or stall ClickHouse. Verify no double-counting on recovery.

Done when: two postmortems exist and the recovery path is proven, not assumed.

---

## Timed-build simulations

First one at the end of Week 3, then roughly monthly.

Eight hours. A problem I did not choose. Empty directory. AI on and used hard.
Ship something that runs and is instrumented, then write up what was cut and
why.

The thing being practised is scoping in the first thirty minutes, not typing
speed. Expect the first attempt to go badly and to be the most informative.

---

## After week 3 — sustainable pace

Drop to 10-15 h/week. Move the deployment to a real Linux box (Hetzner ~€50/mo,
or Oracle Cloud Always Free ARM: 4 cores / 24 GB, genuinely free but often
capacity-constrained). Re-run the benchmark suite there — those are the numbers
you publish, since Docker on macOS distorts I/O.

Then: the product layer (trend and anomaly surfacing, tenant-facing UI), and
the write-ups.

## Portfolio evidence

This project doubles as portfolio evidence: benchmarks, profiling artifacts,
incident postmortems, and defensible architecture decisions are exactly the
kind of thing worth pointing to later. That is a side effect, not the goal —
the goal is the capability itself.
