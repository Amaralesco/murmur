# Week 1 — day by day

Weekly goal: one working path from the Bluesky firehose to a query result,
instrumented, surviving 24 hours unattended. Not a feature set. One event making
it all the way through, provably.

**The constraint that shapes the week:** the 24-hour clock has to start Thursday
evening. Grafana is therefore a Thursday task, not a Friday one.

---

## Monday — done

- [x] Rust toolchain verified (rustc 1.98.0, cargo, rustup)
- [x] `PostHog/posthog` cloned (blobless partial clone, sibling of murmur)
- [x] `lld` installed — their `rust/.cargo/config.toml` requires it and Flox
      normally provides it
- [x] rust-analyzer working, including cross-crate navigation
- [x] Unassisted cold read of `rust/capture` + graded
      (`docs/reading/capture-service.md`)
- [x] Guided read of `capture/src/config.rs`
      (`docs/reading/capture-config-guided.md`)
- [ ] **Carried over:** get PostHog's own stack running locally. Not needed
      until Friday, when reproducing an issue for Track 2 requires it.

---

## Tuesday — Rust ingest

Build the Jetstream WebSocket consumer.

- [ ] Cargo workspace created in `murmur/rust/` with an `ingest` crate
- [ ] `common` crate for shared config and telemetry — do this now, not later
- [ ] Config via `envconfig` derive, read from environment variables
- [ ] Connect to Jetstream, deserialise messages
- [ ] Reconnect loop with backoff
- [ ] Cursor persisted to disk, resumed on startup
- [ ] Raw archive to disk, zstd, **with a retention cap** (105 GB free)

**Hand-written component this week: cursor persistence only.** Typed by me, no
AI codegen. Small on purpose — it forces `Result`, `?`, file I/O and serde
without risking the schedule.

**Done when:** killing the network causes a reconnect, and it resumes from the
correct cursor rather than the beginning.

Warm-up before writing code — the check question still owed from Monday:
`CaptureMode::Import` deliberately skips the global rate limiter. Why would a
system exempt one traffic class from its own rate limiter, and what risk does it
accept by doing so?

---

## Wednesday — Kafka and capture

- [ ] Docker Compose: Kafka, ClickHouse, PostgreSQL, Redis
- [ ] `capture` crate: validate, normalise, produce to `murmur.events`
- [ ] Single partition is fine this week
- [ ] Node worker: consume, batch, write to ClickHouse

**Done when:** an event I can name goes in the socket and lands in a ClickHouse
table.

---

## Thursday — close the loop and instrument

This is the deadline day, not a stretch goal.

- [ ] Worker batching finished, end-to-end path complete
- [ ] Prometheus scraping all services
- [ ] Grafana with four panels: events/sec in, events/sec out, consumer lag, p99
- [ ] Leave it running overnight

**Done when:** the dashboard is live and I have walked away from it.

---

## Friday — measure, ship, defend

- [ ] Confirm 24-hour survival on the dashboard
- [ ] One query returning something true about the last hour
- [ ] Baseline entry in `docs/BENCHMARKS.md` with commit and conditions
- [ ] `docs/DECISIONS.md` entries for this week's choices and rejected options
- [ ] **Architecture defence** — 45 minutes, no notes, no lookups, assistant as
      examiner
- [ ] Track 2: triage one issue labelled `feature/team-ingestion` — reproduce
      and comment, no PR needed

**Done when:** the dashboard proves 24h, the baseline number is recorded, and an
issue is triaged.

---

## Weekend

Slack for overruns, or rest. Do not plan work here — Wednesday will spill.

---

## Jetstream reference (verified 2026-08-31)

**Endpoint (v2, recommended):**

```
wss://jetstream.us-east.bsky.network/xrpc/network.bsky.jetstream.subscribeEvents
wss://jetstream.us-west.bsky.network/xrpc/network.bsky.jetstream.subscribeEvents
```

No authentication for the live tail.

**The gotcha:** the connection requires the WebSocket subprotocol
`xrpc.v1.json`. Without it the handshake fails. In `tokio-tungstenite` this
means building the request with a `Sec-WebSocket-Protocol` header rather than
passing a bare URL.

**Filters** (server-side, so a narrow filter means less bandwidth):

- `collections` — NSIDs, repeatable, wildcards like `app.bsky.feed.*`. Max 100.
- `dids` — repeatable. Max 10,000.
- `kinds` — `commit`, `identity`, `account`, `sync`. Omit for all four.

A `collections` filter constrains **commit events only**; identity, account and
sync flow regardless. A commits-only stream needs `kinds=commit` too.

**Message shape** — an envelope with the event under `payload`:

```json
{
  "$type": "message",
  "payload": {
    "$type": "network.bsky.jetstream.subscribeEvents#commit",
    "did": "did:plc:...",
    "seq": 24664288881,
    "time": "2026-08-13T06:47:43.959305Z",
    "operation": "create",
    "collection": "app.bsky.feed.like",
    "rkey": "3msx2efqdxs27",
    "rev": "...",
    "cid": "...",
    "record": { "$type": "app.bsky.feed.like", "...": "..." }
  }
}
```

The record arrives already decoded — no second parse. A `delete` carries no
`record` and no `cid`, only `collection` and `rkey`, so both fields must be
`Option<T>` in the struct.

**Cursor semantics — this is the important part:**

- `seq` is monotonically increasing. Reconnect with `?cursor=N`.
- The cursor is **inclusive**: reconnecting at `N` redelivers event `N`.
- Delivery is **at-least-once**. Duplicates across a reconnect are guaranteed,
  not exceptional.
- Replay is bounded by a lookback window, so a long outage loses data.
- Handlers must be idempotent. Key on the record's `at://` URI.

**Consequence for murmur:** duplicates are a normal condition, not a bug to be
engineered away at ingest. That is what makes Week 3's "verify no
double-counting on recovery" a real test rather than a formality. Do not try to
make the cursor exact; make the downstream idempotent.

---

## Patterns to copy from Monday's reading

1. Config via `envconfig` derive from environment variables; `default`
   attributes mark optional fields.
2. Custom types parse from strings by implementing `FromStr`; `.parse()` then
   works for free.
3. Use `NonZeroU32` for values that are nonsense at zero, so bad config fails at
   boot rather than at 3am.
4. Fail fast on correctness-critical config, degrade gracefully on optional
   telemetry.
5. Shared concerns (Kafka settings, telemetry) live in a `common` crate from the
   first commit.
6. Match exhaustively on config enums. No `_ =>` arm — let the compiler force
   the decision when a variant is added.
