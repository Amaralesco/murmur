# Task — find out what gets lost on shutdown, then stop losing it

**Status: TODO** (opened 2026-10-14) · **Priority: medium**

Two parts, in order. **Part 1 is investigative and must not be skipped**: there
is a measured discrepancy between what Kafka received and what reached disk, and
the obvious explanation does not account for its size. Fixing shutdown before
knowing what is actually lost would hide the remainder rather than remove it.

`ROADMAP.md` lists graceful shutdown twice — line 30 as a phase 1 item still
open, line 89 as a phase 3 item modelled on PostHog's `common/lifecycle` — and
until now it had no document.

---

# Part 1 — investigate the discrepancy

## What was observed

On 2026-10-14, comparing the archive against Kafka's offset counter for
2026-10-02, the only day on which code produced one message per event:

```
Kafka,  2 Oct:   14,925 total − 15 produced earlier   = 14,910
disk,   2 Oct:   3 + 14,878 across two files          = 14,881
difference:                                                 29
```

Earlier archive files are **not comparable** and must be excluded from any such
count: the files from 15 and 17 September predate a producer existing at all,
and `logs_2026-09-30T15.jsonl` is from slice 3, when one hardcoded message was
produced per *run* rather than per event. A first attempt at this comparison
used all 17,413 archived events and was meaningless.

## Why the obvious answer is not enough

The expected cause is the `BufWriter` on the archive file: writes land in an
in-memory buffer and reach disk when it fills or when the writer is consumed at
rotation. There is no flush on shutdown, so stopping mid-hour abandons whatever
is buffered.

But a `BufWriter::new` default is **8 KiB**, and events average roughly 900
bytes, so that mechanism can account for about **nine** events. The gap is 29.
Three buffers' worth is not a rounding error.

## Hypotheses, and how to tell them apart

| | Hypothesis | Discriminating evidence |
|---|---|---|
| H1 | unflushed buffer on stop | gap is stable across runs and close to `8192 / mean event size`. Measure both rather than assume 900 bytes |
| H2 | the arithmetic is wrong — more runs happened than assumed | `logs_2026-10-02T16.jsonl` holds 3 lines but has an mtime of **17:58**, nearly two hours after its naming hour. That does not fit a single clean run and is the loosest thread here |
| H3 | disk writes failed and were only logged | `handle_message` prints `Could not write to disk` and continues. stdout was not captured, so this cannot be ruled out retrospectively — only forward |
| H4 | events lost during rotation | read the rotation path: a message arriving between the boundary check and the new file being ready has no obvious home. Code inspection settles this one, no experiment needed |
| H5 | Kafka's counter includes messages from something other than `ingest` | `kafka-console-producer` was used by hand during slice 1 and 3 testing. The "15 produced earlier" figure is an assumption, not a measurement |

## The experiment

Controlled, because the historical data cannot answer it.

```bash
# 1. baseline
kafka-get-offsets --bootstrap-server localhost:9092 --topic murmur.events
ls data/readings/

# 2. run for a fixed time, capturing stdout so H3 is visible
cargo run -p ingest 2>&1 | tee /tmp/run.log
#    stop with Ctrl-C after ~60s

# 3. compare
kafka-get-offsets --bootstrap-server localhost:9092 --topic murmur.events
cat data/readings/<files created during the run> | wc -l
grep -c 'Could not write to disk' /tmp/run.log
```

Two variants worth running, because the difference between them isolates H1:

- **stopped mid-interval** — the buffer is abandoned. Expect a gap.
- **stopped just after a rotation** — the writer was consumed and flushed, so
  the gap should be near zero. `ROTATION=minutely` makes rotation happen while
  you watch rather than on the hour.

Repeat the first variant three times. A **stable** gap points at H1 and sizes
the buffer; a **variable** gap points at H3 or H4.

Also worth measuring directly rather than assuming: mean event size.

```bash
# bytes per event in a real archive file
awk '{ total += length($0) + 1 } END { print total / NR }' data/readings/logs_2026-10-02T17.jsonl
```

## Done when

The gap is **explained with a number and a mechanism**, not attributed. Written
down in `docs/BENCHMARKS.md` if it turns out to be a measurement, or as a
postmortem in `docs/postmortems/` if it turns out to be data loss with a cause —
the conventions in `CLAUDE.md` ask for the second whenever a failure is found,
injected or organic.

If the explanation is entirely H1, say so and move to part 2. If any of it is
H3 or H4, that is a separate defect and probably more urgent than shutdown.

---

# Part 2 — stop losing it

Only once part 1 has a conclusion, because the fix differs by cause.

## What exists now

`handle_message` writes each event into a `BufWriter`, and the only flush in the
program happens when a finished archive is consumed at rotation. `main` loops
forever by design — `DECISIONS.md` records that the program never gives up,
since there is no supervisor to restart it — and Ctrl-C terminates the process
wherever it happens to be.

## What that prevents

**"Every event is on disk" is not quite true**, which matters because three
decisions lean on it: the archive is the replay corpus for phase 4, the
backfill source for any schema change, and the durable copy that makes a failed
Kafka send recoverable rather than lost.

**Phase 3's milestone cannot be claimed honestly.** Surviving 24 hours
unattended means stopping at the end, and a stop that drops the tail makes the
run's own output incomplete.

**A controlled measurement is impossible**, which is why part 1 has to work
around it rather than through it.

## Done when

```bash
cargo run -p ingest
# stop it cleanly
```

and the archive contains every event Kafka received — the gap from part 1 is
zero, repeatably, including when stopped mid-interval.

## Decisions owned

- **What "cleanly" means.** Catching SIGINT and SIGTERM is the obvious reading,
  and `tokio::signal` is the tool. Whether SIGTERM alone is enough depends on
  how this is eventually run.
- **What shutdown must do, and in what order.** Flushing the archive is the
  minimum. Finishing an in-flight Kafka send, compressing the current archive,
  and persisting a cursor are all candidates — and the last of those belongs to
  `high/cursor-resume.md`, which has the same shape and the same trigger, so the
  two may want to land together.
- **How long to wait.** A shutdown that blocks forever on a hung broker is a
  different failure. A deadline, and what happens when it expires.
- **Whether to flush more often instead.** Flushing every N events or every N
  seconds shrinks the loss without touching signals, at the cost of syscalls.
  That is a mitigation rather than a fix, and it bounds the problem by time
  rather than eliminating it.

## Pointers

| What | Where |
|---|---|
| Shutdown as a component rather than a signal handler | `posthog/rust/common/lifecycle`, and the notes already taken in `docs/reading/capture-service.md:62` |
| How their services register and await shutdown | `lifecycle::Manager`, used in `capture/src/sinks/kafka.rs` tests |
| The backpressure decision this protects | `docs/DECISIONS.md` 2026-09-01 |

## Out of scope

Cursor persistence (`high/cursor-resume.md`), health endpoints, Prometheus.
Phase 3 owns the last two.
