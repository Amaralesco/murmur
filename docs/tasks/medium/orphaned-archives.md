# Task — archive files abandoned by a restart are never compressed

**Status: TODO** (opened 2026-10-27) · **Priority: medium**

**This reverses a decision.** Slice 3 of `docs/tasks/done/compression-sweep.md`
was cut on 2026-09-18 with the note *"retroactive compression not wanted"*. That
call did not hold: there are now nine orphaned files and 30 MB on disk, and a
24-hour soak run makes it worse per restart rather than better. The reversal is
the point of this document, not a gap being filled.

---

## What exists now

Compression is reached from exactly one place — the rotation branch in
`rust/ingest/src/main.rs`:

```rust
if archive.latest_file_time != Utc::now().format(rotation_format).to_string() {
    info!("rotating to a new archive file");
    match create_file(&config.readings_dir, rotation_format) {
        Ok(next_archive) => {
            let finished = std::mem::replace(&mut archive, next_archive);
            ...
            if let Err(e) = compress_archive(&finished.full_path) {
```

`compress_archive` is only ever called on `finished.full_path` — the file *this
same process* just rotated away from. Nothing enumerates the directory.

And `create_file` opens with `create(true).append(true)`, computing the name
from `Utc::now()`:

```rust
let latest_file_time = Utc::now().format(rotation_format).to_string();
let archive_name = format!("logs_{}.jsonl", latest_file_time);
let file = OpenOptions::new().create(true).append(true).open(&path)?;
```

So which files survive depends entirely on when you restart:

| Restart | Result |
|---|---|
| **Same rotation period** — killed 11:20, restarted 11:30 | reopens and appends to `logs_…T11.jsonl`; compressed normally at the 12:00 rotation |
| **A later period** — killed 11:20, restarted 16:10 | opens `logs_…T16.jsonl`; the `T11` file is **orphaned permanently** |

There is no startup scan, no sweep, and no shutdown hook. An orphaned file is
never named by any code path again.

## What that prevents

**Disk fills with data that compresses 4x.** Measured 2026-10-27: 15,693,140
bytes → 3,892,776 bytes, a **4.03x** ratio. The nine orphans are ~26 MB of
`.jsonl` that would be ~6.5 MB compressed. `murmur/CLAUDE.md` calls disk the
binding local constraint, and this is the one source of waste that grows without
bound.

**It gets worse exactly when it matters.** Measured healthy throughput is
~2,100 events/min, about 15 MB per hourly file. Phase 3 requires a 24-hour
unattended run; every interruption in that run that is followed by a restart in
a later period leaves a ~15 MB file behind uncompressed, forever.

**The archive stops being self-describing.** Nine `.jsonl` and two `.zst` in
`rust/data/readings/` is not a state any rule explains — it is a record of which
Ctrl-C happened to land inside a rotation period. Anything later reconciling the
archive against Kafka, or replaying it as the regression corpus, has to cope
with both forms and with a 0-byte file (`logs_2026-09-30T1559.jsonl`).

## Why now

The 2026-09-18 cut was made with no orphans on disk and no long run attempted.
Both have changed: the 2026-10-27 soak produced a tenth orphan within an hour,
and the compression path is now measured rather than theoretical.

## Done when

```bash
cd rust
# with leftovers present in data/readings/
ls data/readings/*.jsonl        # several

cargo run -p ingest             # run across one rotation boundary
ls data/readings/               # every non-current .jsonl is now .zst
ls data/readings/*.tmp          # nothing
```

Observable claim: **after one rotation, the only uncompressed file in the
directory is the live one.** And every `.zst` passes `zstd -t`.

## The design already exists — do not redesign it

`docs/tasks/done/compression-sweep.md` specifies it in full. Reuse, do not
reinvent:

| Found in the directory | Do |
|---|---|
| the live file (the current period's path) | **skip** — always |
| `X.jsonl.zst.tmp` | delete it |
| `X.jsonl` and `X.jsonl.zst` both present | delete the `.jsonl` |
| `X.jsonl` alone | compress, sync, verify, rename, delete |

Plus, from the same document: sweep **at every rotation** rather than as a
separate step, since right after rotation the finished file is just one more
non-current `.jsonl`; and guard against overlap, because a sweep can still be
running when the next rotation arrives.

## Decisions you own

- **The trigger.** Rotation is what the old document settled on, and it has an
  argument startup does not: compressing gigabytes of leftovers *before*
  connecting the socket would delay the firehose. The cost is that a sweep at
  rotation inherits rotation's gating — see below — so leftovers linger until
  traffic resumes. Startup, shutdown and a timer are the alternatives.
- **Whether it runs on the message loop.** `compress_archive` currently does.
  Measured 2026-10-27: ~4 seconds for a 15 MB file, inline, and the process
  survived. A sweep is N files, so the stall multiplies. `DECISIONS.md`
  2026-09-17 deferred `spawn_blocking` "behind a measured trigger" — this is
  plausibly that trigger, and deciding it here means deciding it once.
- **Files outside `readings_dir`.** Two stale `.jsonl` in `rust/data/tmp/` from
  mid-September. The old document's answer was *"outside the archive directory,
  so the sweep never sees them. Move or delete by hand."* Still the answer, or
  does the sweep get a second directory?
- **The 0-byte file.** `logs_2026-09-30T1559.jsonl`. Compressing it is harmless
  (verify compares 0 to 0 and passes) but it is noise. Skip empties, or let them
  through?

## Overlaps with graceful shutdown

`docs/tasks/medium/graceful-shutdown.md` would remove most of the *cause* rather
than clean up after it: a shutdown that finishes and compresses the current file
leaves no orphan behind. It does not remove the need for a sweep — a kill -9, a
panic, or a laptop that sleeps and never wakes still orphan a file — but doing
shutdown first would shrink this problem to the crash case only.

Worth deciding which order, because doing the sweep first makes the shutdown
work less valuable and vice versa.

## Gotchas, as of 2026-10-27

- **Rotation is gated on frames arriving**, not on the clock — `DECISIONS.md`
  2026-09-14, "bounded by arrival time, not event time". Observed live: the
  10:00 file rotated at **11:18**, because the machine was asleep from 10:48 to
  11:16 and the rotation check sits at the top of the message loop. A sweep at
  rotation inherits this exactly.
- **A sweep must never touch the live file**, and "live" means the path
  `create_file` would compute right now, not the newest file on disk. After a
  same-period restart those are the same file; they are not the same thing.
- **`.tmp` files are never trusted.** The rename to `.zst` only happens after
  verification, so a stray `.tmp` means a previous attempt died mid-flight and
  the bytes are worthless.

## Out of scope

Power-loss durability (the parent directory is not synced after the rename —
`compression-sweep.md` parked this deliberately), changing the rotation period,
retention and deletion policy, and anything about the Kafka side.
