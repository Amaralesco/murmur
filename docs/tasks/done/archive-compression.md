# Task — hourly batch compression of the raw archive

**Status: DONE** (2026-09-18). Kept for the reasoning, not the checklist.

What shipped: rotation on a wall-clock boundary, archive directory from
config, compression with verification before rename, original deleted only
after the check passes. Rotation is hourly via the `ROTATION_FORMAT`
constant in `main.rs` — a constant rather than a config field, deliberately,
since both the file name and the boundary check must derive from one string.

Cut: sub-task 5, the startup scan for orphaned files. Retroactive compression
was not wanted. Consequences accepted — uncompressed orphans from earlier
runs, stale `.tmp` files, and the rare `.jsonl`/`.zst` pair left by a crash
between rename and delete.

Still owed: the retention cap, which `ROADMAP.md` lists among phase 1's open
items and the code does not implement. And the batch-over-streaming choice —
the measurements below — still has no entry in `docs/DECISIONS.md`.

Source of truth: `docs/ROADMAP.md` (phase 1), `docs/DECISIONS.md`
(2026-09-01 backpressure entry).

**Not the hand-written component.** That is item 5, cursor persistence. AI
codegen is allowed here if you ask for it.

---

## Context

**What exists now.** `rust/ingest/src/main.rs:39-46` opens a single hardcoded
`archive.jsonl` in append mode, before the connection loop, and wraps it in a
`BufWriter`:

```rust
let file = OpenOptions::new()
    .create(true)
    .append(true)
    .open("archive.jsonl")?;

let mut archive = Archive {
    writer: BufWriter::new(file),
};
```

`handle_message` writes raw JSON lines into it (`:141-148`). The file never
rotates, never closes, and never compresses. `zstd = "0.14.0"` is declared in
`Cargo.toml` and unused.

**What that prevents.** One unbounded file grows for the life of the process,
so there is no unit you can hand to a replay run, and no point at which
compression could happen. `BufWriter` only flushes on drop, and `main` loops
forever — so nothing ever drops, and the corpus has no defined boundary.

**Why now.** The replay corpus is the input to phase 4's replay harness and the
regression suite behind every `docs/BENCHMARKS.md` number. It has to be a set
of finished, verifiable files before anything can replay it.

**Blocked by.** `main.rs:21` — `pub use crate::stream::Encoder;` does not
compile (`crate::` means `ingest`, and `ingest::stream` does not exist; that
line was copied from zstd's own `lib.rs:45`, where `crate::` means zstd).
Delete it. Batch may not need the crate in-process at all.

---

## The decision this implements

**Batch, not streaming.** Write plain bytes all hour; compress the finished
file at rotation.

Measured on the real corpus (`rust/archive.jsonl`, 1049 events, no repetition):

| Level | Ratio | Throughput | Cost of one 180 MB hourly pass |
|---|---|---|---|
| 1 | 3.57x | 250 MB/s | 0.72s |
| 3 | 3.71x | 218 MB/s | 0.83s |
| 9 | 3.93x | 99 MB/s | 1.81s |

Rate at the current filter (`app.bsky.feed.post` only): **53.7 events/sec**,
932 bytes/event → ~180 MB/hour raw, ~49 MB/hour compressed.

Throughput figures include process-spawn overhead across 200 runs, so treat
them as an upper bound. The sample is 19.5 seconds at 13:29 UTC — too short and
too narrow for `BENCHMARKS.md`; it is good enough to size this decision and
nothing more.

**Why batch won:**

1. The archive's job is to be *inspected*, and storage is explicitly not a
   constraint (87 days to fill the disk at this filter). Streaming optimises
   bytes-on-disk, the axis that does not matter here, and pays in
   inspectability, the axis that does.
2. It leaves the documented backpressure decision undisturbed. An `Encoder` in
   the write path adds zstd's internal ~128 KB block buffering, making the
   blocking behaviour bursty in a way the 2026-09-01 entry did not reason
   about. Batch keeps the write path a bare `write_all` into a `BufWriter`.
3. It separates failure domains — a bug in compression cannot corrupt live
   capture.

**The risk being accepted:** batch needs a *delete the original* step, the only
data-destroying operation in the ingest path. It has to verify the `.zst` is
complete before unlinking. Streaming has no such step. If this decision is
wrong, that is where it will show.

---

## Sub-tasks, in order

1. [ ] Remove `main.rs:21`. `cargo check --workspace` green.
2. [ ] Give `Archive` the state rotation needs — it currently holds only
       `writer`, and cannot answer "when did this file open" or "what is it
       called".
3. [ ] Rotation check on the write path. Note it can only fire when a message
       arrives; a quiet hour will not rotate on its own.
4. [ ] At rotation: flush, close, compress the finished file, remove the
       original **only after verifying the output**.
5. [ ] On startup, find `.jsonl` files left by a previous run and compress them.
6. [ ] Config fields for archive directory and rotation interval, following the
       existing `envconfig` pattern in `ingest/src/config.rs`.

## Decisions you own — settle before coding

- [ ] **Rotation trigger** — wall-clock hour boundary, or an hour of elapsed
      runtime? These diverge badly across restarts, and you will restart often.
- [ ] **Where the pass runs** — inline at rotation (~1s stall, simplest), a
      spawned task, or outside the process entirely.
- [ ] **File naming**, and what the live file is called versus finished ones.
- [ ] **The existing `rust/archive.jsonl`** — 978 KB of real captured data.
      Migrate, compress, or discard?

## Done when

```bash
ls archive/                          # exactly one .jsonl, N .zst
zstd -t archive/*.zst                # every finished file verifies
zstd -dc archive/<one>.zst | wc -l   # plausible line count
zstd -dc archive/<one>.zst | head -1 | jq .payload.seq   # valid JSON
```

Plus two behavioural checks:

- [ ] `kill -9` mid-hour, restart, confirm the orphaned `.jsonl` gets
      compressed — and that the live file was `tail`-able before that happened.
- [ ] No `.zst` is ever left alongside a surviving original.

## Out of scope

**Retention cap.** `docs/ROADMAP.md` still lists one among phase 1's open
items. Deprecated on the grounds that size is not a constraint — a day or a
week of data is enough to work against. **This deferral needs a line in
`docs/DECISIONS.md`** or the docs will keep contradicting the code.

## Follow-ups this surfaced

- `serde_json::from_str(&text).unwrap()` (`main.rs:128`) is still the one panic
  on the hot path. One malformed event from a public firehose ends the process.
  This task's value depends partly on that file being readable when it fires.
- `common/src/telemetry.rs` exists, is untracked, and is **not declared in
  `common/src/lib.rs`** — so it never compiles. Output is still `println!`.
- When this ships, `docs/DECISIONS.md` wants an entry: batch chosen, streaming
  rejected, with the numbers above and the accepted risk stated.
