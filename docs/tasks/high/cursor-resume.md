# Task — resume from a cursor instead of from "now"

**Status: TODO** (opened 2026-09-23)

**Hand-written component.** `.kiro/steering/learning-protocol.md` designates
cursor persistence as hand-written: no code, no pseudocode, no function
signatures from the assistant. Review only, after a first version exists.

---

## What exists now

`rust/ingest/src/config.rs:35-40` builds the subscription URL:

```rust
pub fn jetstream_url(&self) -> String {
    format!(
        "wss://jetstream.{}.bsky.network/xrpc/network.bsky.jetstream.subscribeEvents?kinds=commit&collections={}",
        self.zone, self.jetstream_collections
    )
}
```

There is no `cursor` parameter, and the reconnect path in `main.rs` reconnects
to this same URL. So **every reconnect resubscribes from "now"**, and nothing
tracks the last event processed — not in memory, not on disk.

The reconnect machinery itself is finished and works: capped exponential
backoff, a read timeout derived from the measured 30s keepalive, stall
detection. It reconnects reliably to the wrong place.

## What that prevents

**Every reconnect loses a window of events** equal to the outage. Backoff is
capped at `MAX_DELAY_TIME=500` seconds, so a bad reconnect can lose eight
minutes.

**The loss is invisible.** It cannot be detected from gaps in `seq`, because
`seq` counts every event in the firehose while the subscription is filtered to
`app.bsky.feed.post` — non-contiguous `seq` values are the normal case, not a
symptom. Nothing in the archive distinguishes "no posts happened" from "we
were not listening".

Concretely, this blocks three things already planned:

- **Phase 3's 24-hour unattended run.** A day-long run that silently drops
  every reconnect window proves the process stays up, not that the pipeline
  works.
- **Phase 4's replay harness**, which replays the corpus at a multiplier. A
  corpus with unrecorded holes gives throughput numbers whose denominator is
  unknown.
- **Any claim about completeness** downstream, once events reach Kafka and
  ClickHouse. "Events per hour" is not a true sentence if an unknown number
  were never received.

A restart loses position too, and for the same reason. That is the harder half
and the one the "persistence" in the name refers to.

## When this becomes binding

**Not during phase 2.** At the measured ~53 events/sec, filtered to one
collection and writing about 1 MB/minute, the read loop is nowhere near
saturated, so falling behind is unlikely. None of the pipeline slices are
blocked by this.

It becomes binding at two specific points:

- **Phase 3's milestone — 24 hours unattended.** Over a day, a reconnect is
  likely, and every one silently discards its outage window. A run that loses
  unknown amounts of data proves the process stayed up, not that the pipeline
  works.
- **Phase 4's replay harness**, which replays "the corpus" at a multiplier. A
  corpus with unrecorded holes gives throughput numbers whose denominator is
  unknown, and the archive is also the regression suite.

Also: the moment ClickHouse starts filling, "did we receive everything?" stops
being theoretical, because the answer becomes a number in a query. Rows written
before this is fixed are of unknown completeness, which is worth knowing when
reading early results rather than a reason to stop.

## Why it is more than a missing feature

`DECISIONS.md` 2026-09-01 chose backpressure by blocking — no queue between the
socket read and the disk write — and justified it like this:

> Jetstream's per-connection buffer fills, it disconnects, and
> **reconnect-with-cursor resumes**. Every step is bounded, visible, and
> recoverable.

That recovery step does not exist. So today the sequence is: fall behind,
disconnect, lose the window, and see nothing in the logs or in `seq`. This task
is what makes that decision true rather than aspirational, and the same
reasoning covers any new work added to the blocking path — a Kafka producer
included.

The escalation valve named in that entry is a **bounded** `tokio::sync::mpsc`
between reader and writer, added only when instrumentation shows write latency
stalling the read loop. Unbounded is never an option.

## Done when

The check has to be observable, and the inclusive cursor makes it easy:

```bash
cargo run -p ingest
# note the last seq processed, then break the network for ~60s
# on reconnect, the first event delivered should be that same seq
```

Because **the cursor is inclusive**, a correct resume redelivers the last
event you already had. So the signal for success is a **duplicate at the
reconnect boundary** — not the absence of one. Its absence means the cursor was
not sent, or was sent as `last + 1`.

For the restart half: stop the process, start it again, and the first event of
the new run is the last event of the old one.

## Protocol facts that constrain the design

From `https://bsky.network/docs/jetstream/` and `docs/jetstream.md`:

- `?cursor=N` replays from `seq` N, **inclusive** — event N arrives again.
- Delivery is **at-least-once**. Duplicates across a reconnect are guaranteed,
  not a bug. Handlers must be idempotent, keyed on the record's `at://` URI.
- Replay is bounded by a **lookback window**. A cursor older than the window
  cannot be honoured, so a long outage loses data no matter what is persisted.
  That case needs a decision, not a fix.
- `cursor` also accepts a **unix-microsecond timestamp**, recognised by its
  magnitude, and translated to the nearest `seq`. That is how to resume from a
  point in time rather than a specific event.
- `seq` is assigned by one Jetstream instance. A cursor is not portable to
  another zone, so a persisted cursor is only meaningful with the zone it came
  from.

Downstream, duplicates are already handled: the ClickHouse table uses
`ReplacingMergeTree` with `(jetstream_event_at, uri)` as its sorting key, which
collapses a redelivered event into one row. See `clickhouse/schema-notes.md`.
So resuming with an inclusive cursor costs correctness nothing.

## Decisions this needs, all owned

- **How often to persist.** Every event is safe and slow; every N events or N
  seconds bounds the work and the loss. The bound on loss is what to argue
  about, since a crash loses everything since the last write.
- **Where.** A file beside the archive, or somewhere else entirely.
- **What to write.** A `seq`, or a timestamp, or both — and whether the zone is
  written with it, given that cursors are not portable between zones.
- **What happens when the cursor is outside the lookback window.** Start from
  now and record a known gap, or refuse to start. Silently starting from now is
  what happens today.
- **In-memory versus persisted.** Tracking the last `seq` in the process fixes
  reconnects without touching the disk. Surviving a restart is a separate,
  larger step. They can ship separately.
