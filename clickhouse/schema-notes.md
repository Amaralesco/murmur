# ClickHouse schema — why it is shaped this way

Companion to `schema.sql` in this folder. `schema.sql` is what the database
runs; this file is why. **Update both together** — a change to the DDL with no
change here is a change nobody will be able to defend in a month.

Opened 2026-09-23, slice 2 of `docs/tasks/pipeline-thin-slice.md`.

Nothing here is enforced by a test. Verifying the schema after an image bump or
a schema change is `docs/tasks/low/clickhouse-schema-tests.md`, deferred.

---

## Engine and keys

- **`ReplacingMergeTree(murmur_ingested_at)`** — the argument is the *version*
  column. When a merge finds two rows with the same sorting key, it keeps the
  one with the highest `murmur_ingested_at`, so a redelivery wins over the
  original. Without an argument it would keep whichever it happened to see
  last.

- **`ORDER BY (jetstream_event_at, uri)`** — time first makes "last hour"
  fast; `uri` completes the dedupe key.

- **`PARTITION BY toDate(jetstream_event_at)`** — one partition per day. Also
  the dedupe boundary: a duplicate that straddles midnight survives, which
  reconnect duplicates arriving seconds apart won't do.

- **`DateTime64(6)` on `jetstream_event_at`** — Jetstream's timestamps carry
  microseconds (`06:47:43.959305Z`). Milliseconds would silently truncate.
  `client_created_at` from records only had three digits, so `(3)` is enough.

- **`Enum8` for operation** — stricter than a string: an unexpected value is
  rejected rather than stored. That is the compiler-forces-the-decision
  behaviour preferred in Rust config enums, with the same cost — a fourth
  operation type breaks inserts until an `ALTER`.

- **No `PRIMARY KEY` line** — ClickHouse defaults it to the `ORDER BY`, which
  is what is wanted here.

Two general facts worth keeping in view, because they explain the shape of
everything above:

- A table-level `ORDER BY` is not the `ORDER BY` of a query. It is the
  physical order rows are written in and the basis of the primary index. A
  query still sorts however it likes.
- `ReplacingMergeTree` is not a unique constraint. Nothing is rejected at
  insert. Deduplication happens during background merges, whenever ClickHouse
  chooses, so a `SELECT` immediately after inserting a duplicate sees both
  rows. `SELECT ... FINAL` forces it, correctly and slowly, and has to be
  remembered in every query.

---

## Settled by measurement — `payload.time` survives a replay

The sorting key only deduplicates if `payload.time` is identical when an event
is redelivered. If Jetstream stamped it at send rather than carrying it with
the event, a redelivery would get a new timestamp, the sorting key would
differ, and `ReplacingMergeTree` would never replace anything — at a cost, and
silently.

**The docs do not say, either way** (checked 2026-09-23). The official guidance
goes further and recommends *against* relying on firehose event timestamps at
all, other than for debugging the firehose.

**Measured instead, 2026-09-23.** Connected with the mandatory
`xrpc.v1.json` subprotocol, captured 200 events live, then reconnected to the
same zone with `&cursor=<first seq>` and captured 200 more:

- the cursor is **inclusive** — run B's first event was the cursor itself,
  matching the documented behaviour
- **200 shared events, 0 mismatches.** Every `time` was byte-identical
  between the live delivery and the replay

So `payload.time` travels with the event. `ORDER BY (jetstream_event_at, uri)`
stands.

Note that this conclusion does not depend on the timestamps being distinct
from each other. Had `time` been assigned when an event is *sent*, run B's
stamps would have been minutes later than run A's, because B connected later.
They were identical, so the value travels with the event.

**Second finding — `time` is usually per-event, but not always.** The 200
events in the run above shared a *single* timestamp, which looked at first like
coarse granularity. Two further measurements say otherwise:

| Sample | Events | Distinct timestamps |
|---|---|---|
| the replay run, 2026-09-23 10:10 | 200 | **1** |
| a fresh capture, 2026-09-23 10:16 | 600 | 590 |
| the archive, 2026-09-14 11:55–11:59 | 5357 | 5255 |

So the norm is one timestamp per event, and the single-value run was a
catch-up burst — the server flushing a backlog, everything stamped alike. The
archive shows the mild form of the same thing: mostly clusters of one or two,
with one cluster of 24.

Consequences: `jetstream_event_at` is fine as the leading column of the sorting
key and as the partition expression, but it is **not an identifier**. `uri` is
what makes the sorting key unique, and during a burst that is doing all the
work.

**Third finding — `time` and `rev` are the same grouping.** Measured on the
same two samples:

| Sample | Events | Distinct `time` | Distinct `rev` | Distinct (`time`,`rev`) |
|---|---|---|---|---|
| archive, 2026-09-14 | 5357 | 5255 | 5255 | 5255 |
| the burst, 2026-09-23 | 200 | 1 | 1 | — |

One repo commit produces one `rev` and one timestamp, and can touch several
records. So the timestamp clustering *is* commit batching, and `rev` in the
sorting key would add a column that varies in exactly the places
`jetstream_event_at` already varies. It is stored as an ordinary column and
deliberately left out of the key.

---

## Why not `ORDER BY (uri)` alone

A single-column key is tempting and dedupes *more* — which is the problem. Two
different events share a `uri`:

```
op=create  time=11:58:54.889427Z  rev=3mvi2oc5pix2s  uri=at://did:plc:32e2…/app.bsky.feed.post/3mvi2oabemc2l
op=delete  time=11:59:14.593310Z  rev=3mvi2oveg5o2l  uri=at://did:plc:32e2…/app.bsky.feed.post/3mvi2oabemc2l
```

With `ORDER BY (uri)`, `ReplacingMergeTree` collapses those into one row and
the create is lost, silently. The archive has 38 such records in 4.5 minutes —
about 0.7% of records get a second event.

What the two-column key separates:

| | same `uri` | same `time` | wanted |
|---|---|---|---|
| the same event received twice, after a reconnect | yes | **yes**, measured | collapse |
| two events about one record — create, then delete | yes | no | keep both |

Since `time` and `rev` are one-to-one, "same `time` + same `uri`" means "same
commit, same record", and a commit touches a record once. So a collision in the
sorting key can only be a genuine redelivery. That is the whole argument for the
key, and it is measured rather than assumed.

A concatenated single column built in Rust was considered and rejected: it
would put the timestamp inside a string, so `WHERE jetstream_event_at > now() -
INTERVAL 1 HOUR` could no longer use the index, the key would be larger and
slower to compare, and the partition expression would have to parse the date
back out.

If this ever has to change, it is not free — the sorting key is fixed at
creation. Alternatives that preserve dedupe are `ORDER BY (seq, uri)`, since
`seq` is documented stable across replays, or `ORDER BY (uri)`, which
deduplicates unconditionally and leaves time filters to partition pruning. The
migration is: create the new table beside the old, `INSERT INTO new SELECT ...
FROM old`, verify counts, `RENAME TABLE` to swap atomically.

---

## Not in the table, and why

- **"Type of post" is `collection`.** `app.bsky.feed.post`,
  `app.bsky.feed.like`, `app.bsky.feed.repost`, `app.bsky.graph.follow`. No
  separate column is needed. The one thing `collection` does *not* tell you is
  post versus comment: a reply is also `app.bsky.feed.post`, and what makes it
  a reply is a `reply` field inside the record. That belongs with `text`, in
  the post-shaped table below, not here.

- **A separate table for post-shaped fields.** `text` is only ever set for
  `app.bsky.feed.post`, and the same will be true of `reply`, `langs`,
  `embed`. Keeping them here means they are null for every like and follow.
  The plan is a second table keyed on `uri`. Deferred on purpose: one table
  has to work first.

- **Alerting on the clock gaps.** The table's job is to store the three
  timestamps. Thresholds and alerts are phase 3, in Grafana, not here.

- **`uri` is stored rather than derived.** It is redundant —
  `did`, `collection` and `rkey` are all present, so `ORDER BY (did,
  collection, rkey)` would dedupe identically with nothing concatenated.
  Stored because it is built in Rust anyway and is the natural key to join a
  second table on. ClickHouse could also derive it on read with an `ALIAS`
  column, storing nothing.

---

## What Rust does not yet send

As of 2026-09-23, `rust/ingest/src/main.rs` parses only `record`, `cid`,
`did`, `seq` and `operation`. Four columns have no source yet:

| Column | Needs |
|---|---|
| `collection` | a new field on `Commit` |
| `rkey` | a new field on `Commit` |
| `rev` | a new field on `Commit` |
| `jetstream_event_at` | `payload.time`, a new field on `Commit` |
| `client_created_at` | `record.createdAt`, a new field on `Record` |

`uri` cannot be built at all until `collection` and `rkey` are parsed.

Two couplings from `docs/jetstream.md` still apply: `payload: Commit` is only
sound while the URL hardcodes `kinds=commit`, and `Record.text` is tied to
`collections=app.bsky.feed.post`.
