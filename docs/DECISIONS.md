# Decisions

Every architectural choice, including the options rejected and why. If it is
not written here, I will not be able to defend it a month later.

---

## 2026-08-31 — Reweighted the learning protocol away from unassisted reading

**Context.** The original protocol gave ~25% of the week to unassisted code
reading, justified by the idea that the hardest thing to practise was reading an
unfamiliar codebase from cold with no tools.

**Finding.** That is the wrong thing to optimise for. Reading cold is a narrow
skill; the capabilities that actually compound are reasoning about design out
loud and building something real from zero under time pressure. Those are what
both good engineering day-to-day and open-ended technical interviews exercise,
and neither is served by grinding unassisted reading. Reading their code with AI
on is more educational per hour than reading it cold.

**Decision.** Reading drops from ~25% to ~15% and loses its dedicated day.
Architecture defence is promoted to a weekly Friday ritual with the assistant
as examiner. A periodic eight-hour greenfield build simulation is added.

**Rejected: delete unassisted reading entirely.** The case for cutting it is
real, but the request to reduce it arrived the same afternoon as a request to
skip it out of reluctance. Keeping one 25-minute graded sitting per week costs
almost nothing and guards against having talked myself out of the hard thing.
The remaining justification is narrow but genuine: I need an independent model of
Rust to judge AI-written Rust, and reading speed compounds.

**Rejected: switch the project to Go.** Go would widen the market for
high-throughput backend roles generally, but PostHog's ingestion path is Rust
with a Node/TypeScript worker, and no Go was found anywhere in their stack.
Since PostHog is the reference codebase this project studies, switching to Go
would throw away the answer key. Revisit only as a deliberate strategic choice,
not because Rust felt heavy on a given afternoon.

---

## 2026-09-01 — Node/TypeScript for `worker` and `api`: one of the two original reasons is dead

**Context.** `architecture.md` justified Node for the worker on two grounds:
practising both languages is a goal of the project, and "PostHog's real worker
is Node".

**Finding.** The second is out of date. Their repo now contains both
`rust/ingestion-consumer` and `nodejs/src/ingestion/ingestion-consumer.ts`.
Person logic exists in `nodejs/src/ingestion/common/persons` *and* in a cluster
of Rust crates (`personhog-identity`, `-router`, `-writer`, `-leader`,
`-replica`, `-coordination`), with generated protobuf bindings on the Node side
under `nodejs/src/common/generated/personhog`. That is a migration in progress:
Node still runs the pipeline but increasingly delegates to Rust services.

**Decision.** Keep Node/TypeScript for `worker` and `api`, on the first reason
alone — practising both languages is a goal, and both should be exercised.

**Rejected: move the worker to Rust to match where they are heading.** It would
enlarge the Rust surface at exactly the point where finishing is the binding
risk, and `architecture.md` already forbids rewriting the worker in Rust without
a measurement showing it is the bottleneck.

**Note for the architecture defence.** If asked why the worker is Node, the
honest answer is now "to demonstrate both languages, and because the stateful
enrichment work is not the hot path" — not "because that's what PostHog does".

---

## 2026-09-01 — Rust stays, with a tripwire

**Context.** Recurring doubt about whether Rust is the right choice given no
prior independent Rust experience. Go is easier to become productive in.

**Decision.** Rust stays for `ingest` and `capture`, scoped as small as possible
so the project remains finishable.

**Why.** Their ingestion path is Rust and becoming more so; no Go was found
anywhere in their stack, so Go would be preparation for a different job. And for
the hot path specifically, Go's garbage collector hides allocation cost, which is
one of the things this project exists to make visible.

**The real risk is not difficulty, it is non-completion.** A working Go pipeline
would beat a half-finished Rust one, because the value here is a system that runs
continuously with numbers that can be defended.

**Tripwire.** If no event has reached ClickHouse by the end of Week 1, the
language choice is reopened — with evidence, not with friction. Go has now come
up twice, both times immediately after a hard stretch. The tripwire exists to
distinguish a bad decision from a hard afternoon.

**Superseded in part by 2026-09-09.** The "non-completion is the real risk"
framing above no longer holds; learning is the validator. The Rust decision and
the tripwire itself both stand — only what the tripwire *means* when it fires
has changed.

---

## 2026-09-01 — Backpressure by blocking, not buffering

**Question.** When the archive writer cannot keep up with the socket: drop
events, buffer in memory, or stop reading and let Jetstream absorb it?

**Decision.** Block. No queue between the socket read and the disk write.

**Why dropping is out.** The archive is also the replay corpus, and the
non-negotiables require every benchmark number to be reproducible against it. A
corpus with silent holes is not a regression suite.

**Why buffering in memory is out.** An unbounded buffer converts a throughput
problem into an out-of-memory kill on a delay. When it happens the process is
killed by the kernel with no chance to flush, losing both the buffered events and
any unpersisted cursor progress. It relocates the problem to the least
recoverable place available.

**Why blocking is right, and cheap.** A synchronous write means the read loop is
inside the write call and not draining the socket. Jetstream's per-connection
buffer fills, it disconnects, and reconnect-with-cursor resumes. Every step is
bounded, visible, and recoverable — and it reuses the reconnect and cursor
machinery that has to exist anyway. Blocking costs no new code.

**Why we can do this and PostHog cannot.** Their `capture` is an HTTP server and
must return 200 quickly or client SDKs retry and amplify the problem. You cannot
apply backpressure to a browser, which is why their answer is rate limiting and
overflow topics instead. `ingest` is a *client* with a resumable cursor, so
backpressure is available and is the cheaper answer. This is the main reason
`ingest` is not a copy of their `capture`.

**Escalation trigger.** Add a **bounded** `tokio::sync::mpsc` channel between
reader and writer only when instrumentation shows write latency stalling the read
loop. A bounded channel is blocking with a shock absorber — it smooths jitter (a
brief disk stall, a log rotation) without disconnecting, while sustained slowness
still reaches the socket. Unbounded is never an option.

Note: a `BufWriter` on the file is unrelated to this decision. It batches
syscalls, does not grow without bound, and should be used.

---

## 2026-08-31 — Config via environment variables, not files

**Decision.** All services read configuration from environment variables.

**Why.** It is the convention for containerised services, and it is what
PostHog does — `capture` uses the `envconfig` crate and `Config::init_from_env()`.
Matching the reference implementation makes their code a usable guide.

**Rejected: config files.** Simpler to inspect locally, but needs a separate
mechanism for secrets and per-environment overrides in Docker and Kubernetes.

---

## 2026-09-02 — Shared `common` crate built now, before its second consumer

**Context.** `murmur-common` is a library for shared config and telemetry, but
only one binary exists (`ingest`). `capture` is not built yet, so "shared" code
currently has exactly one consumer.

**Decision.** Put config and telemetry in `murmur-common` now, as `WEEK-1.md`
prescribes, rather than keeping them local to `ingest` and extracting later.

**Why.** Shared concerns get a home before there is any pressure to copy-paste
into a second binary. Explicitly also a learning decision: building the
abstraction early and then watching whether it was the right shape is more
instructive than avoiding the question, and it stands as a reminder to keep
checking whether `common` has earned its contents.

**Rejected: extract on second use.** The standard discipline — duplicate first,
centralise when the same concern actually appears twice — is the safer default,
because duplication is cheaper to fix than a wrong abstraction. Rejected
deliberately, with the risk accepted rather than overlooked.

**The risk being accepted, stated plainly.** With one consumer, what belongs in
`common` is a prediction, and Rust gives no feedback if the prediction is wrong:
`pub` items in a library crate are never flagged as dead code, so anything
misplaced here stays silently unused. No tool detects this; it is a manual
review.

**Review trigger.** When `capture` is built, check every item in
`murmur-common`: if `capture` does not use it, it was `ingest`-specific and
should move back.

---

## 2026-09-09 — Learning is the validator, not completion

**Context.** Two framings have been running side by side without being
reconciled. The 2026-09-01 entry above states it plainly: *"The real risk is not
difficulty, it is non-completion. A working Go pipeline would beat a
half-finished Rust one."* `ROADMAP.md` backs it with a tripwire, and
`project-context.md` defines a phase as done when there is a working end-to-end
path. Meanwhile `learning-protocol.md` insists the point is capability, forbids
AI codegen on the designated component, and requires that I be able to explain
every line before it ships. The first framing rewards getting there; the second
rewards how I got there.

Under schedule pressure those give opposite instructions, and it is exactly
under schedule pressure that the instruction matters.

**Decision.** **Learning is the validator.**  Falling behind `WEEK-1.md` or `ROADMAP.md` is not
failure — they pace the work, they do not oblige it. Scope may be cut freely;
*how* something got built may not be shortcut. Benchmarks, postmortems and the
dashboard are evidence that learning happened, not the object of the exercise.

**The one thing completion still protects.** Something has to actually run, and
keep running, because a system that never runs teaches nothing about operating
one — and operational pain is a stated non-negotiable. That is the entire
remaining case for finishing anything. It is a real case, and it is narrower
than "ship the roadmap".

**Rejected: completion as the validator**, i.e. leaving the 2026-09-01 framing
in force. The argument for it is not weak. A project that never runs produces no
benchmarks, no postmortems, and no defensible numbers, and "I learned a lot"
with nothing running is the classic self-deceiving outcome. Rejected anyway,
because the failure mode it guards against is not the one actually in play: the
observed risk here has been the opposite — reaching for a higher tier of help to
stay on schedule, which yields code on time that I cannot defend. That is the
signature failure `learning-protocol.md` exists to prevent, and a
completion-first framing licenses it.

**Rejected: deleting the Week 1 tripwire.** It survives, with its meaning
changed. It no longer reads "Rust failed, switch to Go." It reads: *stop and
review, with evidence.* Its original justification stands untouched — Go has
come up twice, both times immediately after a hard stretch, and the tripwire
exists to distinguish a bad decision from a hard afternoon. Demoting completion
does not make that guard less necessary; if anything it makes it more so, since
"learning is the validator" is an easy thing to hide behind.

**Consequence to watch.** The honest risk being accepted is that this entry can
be used to excuse drift. The countermeasure is that the tripwire still fires and
still demands evidence, and that the Friday architecture defence is unchanged —
it is graded on whether I can defend the design, which is not something a
slipped schedule can fake.

---

## 2026-09-14 — Archive files are bounded by arrival time, not event time

**Context.** `ingest` starts a new archive file on a wall-clock boundary. The
check compares `Utc::now()` — when *this process* notices — against the boundary
stored on the open file. The events themselves carry `payload.time`, which is
set at Bluesky when the event happened.

Those are two different clocks on two different machines, with network latency
and server-side queueing between them. An event stamped `11:58:00.010` can
arrive while the local clock still reads `11:57:59.98`, and lands in the file
named for the earlier minute.

**Measured.** First real rotation run, 2026-09-14, four files at one-minute
granularity: two files were clean, one carried 2 stray events out of 825
(0.24%), one carried 1 out of 2392 (0.04%).

**Decision.** Accept the drift. **A filename records when the file was opened,
not what is inside it.**

**Why this is not merely an off-by-one.** The rotation check currently runs
before `stream.next().await`, which adds one message of lag — but moving it
after the read would not close the gap. The gap is the distance between two
clocks, and no placement of a local check removes it.

**Rejected: bucket on the event's own timestamp.** This would make filenames
exactly describe contents, which is the property a partitioned query wants. It
requires a closed file to be reopened when a late event arrives, or a watermark
scheme that holds a window open for some grace period and accepts data loss past
it. That is the same problem every streaming system has, and it is a much larger
design than the archive needs. Revisit only if something downstream genuinely
requires "every event in window N is in file N".

**What this is fine for, and what it is not.** Fine for the replay corpus, whose
requirement is "roughly N minutes of traffic per file, in order, with no holes".
Not fine for anything that partitions by file and assumes the boundary is exact.

**Note on the current interval.** Rotation is presently at minute granularity
(`%Y-%m-%dT%H%M`) so the behaviour can be observed in a short run. That is a
debug setting, not the intended interval, and the drift percentages above will
fall proportionally once the window is an hour.

**Still owed.** The choice of batch compression over streaming — made
deliberately, with measured numbers — currently lives only in
`disposable/archive-compression.md`, which is gitignored and disposable. It
needs its own entry here when compression actually ships.
