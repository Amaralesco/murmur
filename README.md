# murmur

A multi-tenant event ingestion and analytics pipeline built on the Bluesky
firehose. A "project" is a filter over the firehose — keywords, accounts,
record types — and the pipeline turns that slice into real-time analytics.

It is a vehicle for learning Rust by building something that has to actually
run: a live socket, a disk archive, a broker, a columnar store, and the
failure modes that come with all four.

## Layout

```
rust/              Cargo workspace — all cargo commands run from here
  common/          murmur-common: shared config and telemetry
  ingest/          the Jetstream client: socket, reconnect, archive
docs/              DECISIONS, ROADMAP, BENCHMARKS, jetstream reference, tasks
.kiro/steering/    how the project is built, and how the assistant behaves
```

## Running it

```bash
cd rust
cargo run -p ingest        # reads rust/.env
```

`ingest` connects to Jetstream, archives every event to disk as JSON lines,
rotates the archive on the hour, compresses each finished file with zstd and
verifies it before deleting the original.

## Where things are written down

- **`docs/DECISIONS.md`** — every architectural choice, the options rejected,
  and why. If it is not there, it cannot be defended a month later.
- **`docs/ROADMAP.md`** — the phases and where the work currently is.
- **`docs/BENCHMARKS.md`** — every performance claim, with the commit and the
  conditions it was measured under.
- **`docs/tasks/`** — the current stories and their state.
- **`docs/jetstream.md`** — protocol facts that are easy to get wrong.
- **`.kiro/steering/learning-protocol.md`** — how this project is built and how
  the assistant is expected to behave. The authority for the five levels above,
  plus the "state the problem before the task" rule, the modification test and
  the architecture defence.
- **`.kiro/steering/architecture.md`** — component map, language boundaries,
  infrastructure stance.

## How this project is worked on

murmur is a learning exercise that happens to be a real system. **The output is
understanding, not shipped code** — a half-built component I can defend beats a
complete one I cannot, and falling behind the plan is not failure. The one
thing worth protecting is that it keeps running, because a system that never
runs teaches nothing about operating one.

That makes assistance a question of *which kind* of thinking gets outsourced,
not how much:

- **Syntax is unrationed.** Crate features, trait bounds, borrow-checker
  errors — vocabulary, not thinking. Answer freely.
- **Design is never given.** Where a struct lives, what overflow does to
  ordering, how often the cursor persists. Ask, let me answer, then push back.
- **Hand-written components** are typed by me with no AI codegen. Review only,
  after a first version exists. Currently: cursor persistence.

### The five levels of help

I name a level when I ask. **Default to the lowest that could work, and never
escalate unasked.**

1. **Warmer or colder** — am I on the right track?
2. **Name the concept**, don't explain it. I'll go read.
3. **Point at a file and line in `../posthog/`** that solves this. ← the one to
   reach for
4. **Explain with an example from another domain**, not my code.
5. **Write it.**

Level 3 works best: there are ~98 crates of production Rust next door solving
these exact problems, and using them as an answer key beats prose.

**`.kiro/steering/learning-protocol.md` is the authority** — this is a summary.
It also covers the "state the problem before the task" rule, the modification
test, and the architecture defence.

## Non-negotiables

1. **Instrumented before optimised.** Never optimise without a measurement
   identifying the bottleneck.
2. **Runs continuously against the live firehose**, not as a demo started by
   hand. The operational pain is the point.
3. **Every number is reproducible** against the recorded replay corpus.
4. **Real failures get written up** in `docs/postmortems/`.
5. **Every line must be explicable.** If a design decision cannot be defended
   without assistance, it does not ship.

`../posthog/` is a read-only clone of PostHog, used as a reference for
production Rust. It is never edited.
