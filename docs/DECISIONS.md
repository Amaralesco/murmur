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
