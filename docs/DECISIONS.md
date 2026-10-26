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
---

## 2026-09-17 — Compression runs inline on the message loop, not in a background task

**Context.** At rotation, `ingest` compresses the finished archive and verifies
it by decompressing it again. Both happen on the same task that reads the
WebSocket, so the read loop is paused for as long as they take. The alternative
is handing the work to `tokio::task::spawn_blocking` and returning to the socket
immediately.

**Measured**, on the real corpus rather than synthetic data:

| What | Result |
|---|---|
| zstd throughput, level 1 / 3 / 9 | 250 / 218 / 99 MB/s |
| Real archive compressed | 864,619 → 239,682 bytes (3.61x), `zstd -t` passes, `Check: XXH64` present |
| Event rate at `collections=app.bsky.feed.post` | 53.7 events/s, 932 bytes/event → ~180 MB/hour raw |
| Jetstream keepalive ping interval | 30.00s ± 0.07, both idle and while carrying 5,311 events per 100s |
| Server's patience for an unanswered ping | connection closed ~5s later |

Throughput figures come from 200 single-threaded runs including process-spawn
overhead, so they are a lower bound on speed and an upper bound on time.

**Derived, not measured.** A 180 MB hourly file compresses in ~0.83s. The
verification pass decompresses the same content, which is faster than
compressing it — call it a few tenths of a second. **~1.1s per rotation**, at
hourly volume. At the current minute granularity the files are roughly twenty
times smaller.

**Decision.** Run both inline. ~1.1s against a ~5s budget before Jetstream
gives up on us is four to five times the margin, and the pause loses nothing:
messages queue in the socket and server buffers and are read when the loop
returns, exactly as the 2026-09-01 backpressure entry describes.

**Rejected: `spawn_blocking` now.** It would cost a `'static` closure with
cloned paths, a guard so two compressions cannot overlap at the next rotation,
and a way to surface errors from a task nobody awaits. All of that buys an
improvement too small to measure at current volumes.

**Revisit when any of these is true:**

- a rotation stall is **measured** above ~2s
- `JETSTREAM_COLLECTIONS` is widened beyond `app.bsky.feed.post`, which
  multiplies file size by an unknown but large factor
- the compression level is raised above 3 (level 9 is less than half as fast)
- files grow while cursor persistence is still unbuilt

**The risk being accepted.** If a stall ever does exceed the budget, Jetstream
disconnects, and until cursor persistence exists the reconnect resubscribes from
"now" — so the gap is lost rather than replayed. That consequence, not the
latency, is what makes the trigger worth watching.

**Also worth recording.** Verification roughly doubles the I/O per rotation: the
compressed file is written, then read back and decompressed in full. That is the
price of never deleting an original on the strength of an unchecked `.zst`.

---

## 2026-09-18 — The project is for learning Rust, not for a job

**Context.** `project-context.md` opened with "deliberate skill development
toward an ingestion pipeline engineer role (PostHog-style: billions of
events/month, Rust + Node, Kafka, ClickHouse, PostgreSQL, Redis)". Nearly every
other choice inherited from that sentence: the language split, the contribution
track filtered to one team, the interview-shaped rituals, the week numbering.

That is no longer the aim. The goal is learning Rust.

**Decision.** Learning Rust is the purpose, and the docs now say so.

**Consequence: the consumer is Rust, and Node is deferred.** The 2026-09-01
entry kept Node for the `worker` on one surviving reason — practising both
languages — having already discarded the other as out of date. Remove the role
that wanted both languages and nothing holds Node up. `architecture.md` now
reads "Rust everywhere", with TypeScript available later as its own exercise if
an API layer warrants it.

**Consequence: `project-context.md` is dissolved.** Its product description
moved to a new `README.md`, which the repo did not have. Its non-negotiables
and its definition of a finished phase moved to `learning-protocol.md`, where
they are enforceable. The job framing is deleted.

**Consequence: the schedule is gone.** `WEEK-1.md` was a day-by-day plan whose
days expired weeks ago; it is deleted, and its Jetstream protocol notes — which
are still correct and still easy to get wrong — are extracted to
`docs/jetstream.md`. `ROADMAP.md` is recut into phases. Named days are removed
from `learning-protocol.md`; the rhythm survives as read, build, close.

**Kept: Track 2 and the rituals.** Contributing to `PostHog/posthog` is good
Rust practice whether or not a job is in view, so it stays, minus the filter to
one team. The architecture defence stays because defending a design out loud is
how you discover whether you understand it. The timed build stays because
scoping under pressure is a skill in itself.

**Rejected: keeping Node for the worker.** The cost of dropping it is real and
accepted: the project will not demonstrate Node, and PostHog's
`nodejs/src/ingestion/` stops being usable as an answer key, since you cannot
read a reference implementation in a language you do not write. Set against
that, this story's subject is Kafka and ClickHouse, and stacking a third
unknown on top would dilute it.

**Rejected: rewriting the history in this file.** Earlier entries mention days,
weeks and a job target because that is what was true when they were written.
Editing them would make the record dishonest and destroy the reason the file
exists.

**Note on an older objection.** The 2026-09-01 entry resisted enlarging the Rust
surface because non-completion was the binding risk. 2026-09-09 already replaced
completion with learning as the validator; with Rust itself now the goal, a
larger Rust surface is the point rather than the risk.

---

## 2026-09-21 — Apache Kafka, not Redpanda

**Context.** Slice 1 of the pipeline story needs a broker running locally, and
`docs/tasks/done/infra-kafka-clickhouse.md` left the choice open. Redpanda and
Apache Kafka both speak the Kafka wire protocol, so any client works against
either and the choice is reversible at the Compose file, not in application
code.

**Decision.** Apache Kafka, in KRaft mode — no Zookeeper.

**Reason.** Kafka is far more widely used, which makes it the more valuable
thing to learn. Every article, Stack Overflow answer, book and war story
assumes Kafka, so what is read transfers directly to what is running. That
alignment is the whole reason, and under a protocol where learning is the
validator it outweighs operational convenience.

**Rejected: Redpanda.** The case for it was real and is what PostHog runs in
dev (`redpandadata/redpanda:v25.1.9`): a single C++ binary, no JVM, no
Zookeeper, faster to start and with far fewer knobs to get wrong. Redpanda is
not Kafka — it is an independent reimplementation of the protocol — so
learning it would have meant learning a compatible substitute rather than the
thing itself.

**Cost accepted.** More configuration surface, a JVM, and slower startup. The
larger cost is to the answer key: `posthog/docker-compose.base.yml:209-231` is
no longer a line-for-line reference for the broker service, since its flags are
Redpanda's. What still transfers is the part that matters — the two-listener
structure that makes the broker reachable from the host, which is a property of
the Kafka protocol and not of either implementation.

---

## 2026-09-21 — One Compose file, not a dev/prod split

**Context.** PostHog keeps eight Compose files over a shared
`docker-compose.base.yml`, each environment adding what is specific to it —
which is why `base.yml` declares no `ports` for ClickHouse and
`dev.yml:504-508` does. Publishing a port is an environment decision, not a
property of the service.

**Decision.** One `docker-compose.yml`. No dev/prod split.

**Reason.** murmur has no production. There is no server and no deployment
target; the pipeline story ends with a SQL query on a laptop. Eight files
answer a problem of many environments and dozens of services, and neither
exists here.

**Rejected: a `prod.yml` identical to `dev.yml`.** The case for it was that
having both would show the question had been considered. This file is that
evidence, and it is better evidence, because it holds the reasoning where a
duplicate holds none. A second file that nothing ever runs also drifts
silently — dev changes, prod does not, and nothing fails to reveal it.

**Revisit when** the same service needs two configurations at once, or a real
deployment target appears. The split earns its keep only once the files
genuinely differ, and the differences are known: which ports are published to
the host, the `restart` policy, resource limits, and whether the data volume is
a bind mount or a named one.

---

## 2026-09-22 — No authentication and no TLS on the local stack

**Context.** R6 of slice 1 asks for this to be written down, so that the
security question reads as answered rather than forgotten. Both services
needed a deliberate setting to get there. Kafka's listeners are `PLAINTEXT`.
The ClickHouse image goes further than doing nothing: at startup it writes
`users.d/default-user.xml`, which restricts the passwordless `default` user
to connections from inside the container. From the host the result is
`Authentication failed`, while `/ping` still answers `Ok.` — so the lockdown
is invisible to a healthcheck and first appears as a query error.

**Decision.** No authentication and no TLS. Kafka stays `PLAINTEXT`.
ClickHouse sets `CLICKHOUSE_SKIP_USER_SETUP: 1`, which stops the startup
script writing that file and lets `default` in from anywhere without a
password.

**Reason.** The stack runs on one laptop and is never exposed. SASL and
certificates would cost a day and teach nothing about ingestion, which is
what this story is for.

**Rejected: credentials for ClickHouse (`admin` / `admin`).** Considered and
dropped the same afternoon. The costs were concrete: a password committed in
the Compose file, and a user and password carried through `config.rs` from
slice 2 onwards, for a stack that never leaves the machine.

**Condition.** No auth is safe only while nothing outside the laptop can
reach the ports. That is not yet true. The Compose file publishes on all
interfaces — `"8123:8123"`, `"9092:9092"` — which binds `0.0.0.0`, so
anything on the same network can reach both services unauthenticated.
Prefixing the mappings with `127.0.0.1:` is what makes this decision sound.

**Revisit when** anything faces a network: a second machine, a cloud
deployment, a shared environment. `CLICKHOUSE_SKIP_USER_SETUP` and the
`PLAINTEXT` listeners are the two switches to flip.

---

## 2026-09-28 — `capture` is not a separate service; `ingest` produces

**Context.** `.kiro/steering/architecture.md` splits the produce side in two:
`ingest` owns the socket, cursor management and the raw archive, and a separate
`capture` validates, normalises, routes by tenant, detects hot keys and produces
to Kafka. That split arrived in the initial commit and has no entry in this
file — the only structural claim in the project with no recorded reasoning.
Slice 3 needs a producer, which forced the question.

**Finding: the reference implementation does not do this.** PostHog's `capture`
binds its own listener (`posthog/rust/capture/src/main.rs:128`) and holds
`kafka.rs`, `s3.rs` and `fallback.rs` as sink *modules* in
`capture/src/sinks/`. One process owns intake, validation, producing and
archiving. There is no service in front of it, and every process boundary in
that workspace — `ingestion-consumer`, `kafka-deduplicator`, `property-defs-rs`
— sits downstream of a Kafka topic.

**Decision.** One process owns intake, archive and producing. The
responsibilities assigned to `capture` become modules inside `ingest` as they
are needed. The consumer in slice 5 is a separate crate.

**Reason.** Split processes where a queue already separates them. A Kafka topic
is a transport, a buffer and a retry boundary acquired for free. Between a
socket reader and a producer there is none, so splitting means designing one —
and what it should carry depends on what `capture` would do, three of whose four
jobs do not exist yet. Deferring the boundary means designing it with
information rather than guesses.

**Rejected: keeping the split.** The advantages were real and are recorded
honestly. A separate producer process cannot delay the socket reader, which
matters because lost firehose data is unrecoverable. It could be restarted
without dropping the Jetstream connection. The archive would have been a
ready-made queue, leaving `ingest` with only the loss-sensitive job of getting
bytes to disk. And validation would have had an obvious home instead of
accreting in the read loop.

Against that: a transport designed at the point of least information; a
component whose only content today is `send()`; two processes to run, configure
and keep alive before anything works end to end; and pulling in the
`murmur-common` audit and telemetry, both deliberately parked.

**Consequence: the backpressure dependency now covers producing.** The
2026-09-01 entry chose blocking over buffering because "Jetstream's
per-connection buffer fills, it disconnects, and reconnect-with-cursor
resumes". Cursor resume is not implemented, so that recovery step does not
exist, and a producer on the blocking path is a second way to fall behind.
`docs/tasks/high/cursor-resume.md` is promoted accordingly, and `ROADMAP.md`
names it a prerequisite of phase 3's 24-hour milestone. The escalation valve is
unchanged: a **bounded** `tokio::sync::mpsc`, added only when instrumentation
shows the write path stalling the read loop.

**Consequence: a review trigger has to move.** The 2026-09-02 entry set the
`murmur-common` audit to happen "when `capture` is built". Since `capture` will
not be built, that trigger would never fire. It moves to the slice 5 consumer
crate — the second consumer arriving by a different door than expected.

**Consequence: `architecture.md` is now wrong.** Its component map still shows
`ingest` → `capture` → Kafka, and separately still labels `worker` and `api` as
Node/TS, which the 2026-09-18 entry replaced with Rust everywhere. It needs
updating; this entry is the reasoning it should point at.

**Update, same day.** `architecture.md` is corrected: the map folds producing
into `ingest`, names the four responsibilities as modules there, states the
"split where a queue already separates them" rule, and labels `worker` and `api`
as Rust. Two dead triggers were found and repaired at the same time — telemetry
(`docs/tasks/done/tracing-logging.md`) was deferred "until `capture` exists"
and would have been deferred forever; it and the `murmur-common` review now hang
off the slice 5 consumer crate, which preserves the original reasoning that both
should land when a second binary is born.

**Revisit when** hot-key routing needs state, a second or third topic is
wanted (`overflow`, `historical`), or instrumentation shows the producer
stalling socket reads. Keeping the producer in its own module, with no Kafka
types in the message loop, is what keeps that move a file move.

---

## 2026-10-26 — Boot wiring is shared through a generic `bootstrap`, not duplicated

**Context.** Telemetry was deliberately deferred until a second binary existed,
so that the new one would be born with it and `ingest` would be converted
alongside rather than twice. The consumer crate is that second binary. Wiring it
up meant four statements in `main` whose order is forced by data dependencies:
load `.env`, build `Config`, install the subscriber using the level from that
config, then report whether `.env` loaded. Only the middle step is
crate-specific — `ingest::config::Config` and `ingestion_consumer::config::Config`
are different types — and it sits in the middle of three shareable ones.

**Decision.** `murmur_common::telemetry::bootstrap<C>()`, generic over
`C: Envconfig + HasLogLevel`, returning `anyhow::Result<C>`. `HasLogLevel` is a
new trait in the same module with one method, implemented in four lines by each
binary's `Config`. Each `main` is then one line.

**Why a trait is unavoidable, not a preference.** `murmur-common` cannot name
either `Config` type — the dependency arrow runs from the binaries to `common`,
and reversing it is a cycle Cargo rejects. A shared function that takes "the
config" therefore cannot name a type; it has to name a capability. Rust's
coherence rules then put the `impl` in the binary's crate, which is why the
boilerplate is per-binary and cannot be centralised.

**Rejected.** Sharing only the `dotenvy` call: pays for an indirection and still
leaves three statements in each `main`. Sharing the load plus a deferred report
object: exists only to carry the dotenv outcome across the `init` call, which a
different ordering deletes outright. Having `telemetry::init` read `LOG_LEVEL`
from the environment itself, so the subscriber could go up before config: the
cleanest boot and the only shape that can log a config failure, but it removes
`LOG_LEVEL` from both `Config` structs, so the process reads a variable no struct
declares — losing "read one file to see everything this binary takes from the
environment", which the config convention exists to protect. Not sharing at all,
which is what PostHog does across 27 binaries, seven of them with a hand-copied
private `setup_tracing` and no shared helper: defensible, and declined because
one of the two call sites did not exist yet, so the duplicate would have been
written rather than inherited.

**Consequence: a config failure cannot be a log line.** `bootstrap` returns
before the subscriber exists when `C::init_from_env()` fails, so the error
reaches `main` as a value and `anyhow` prints it to stderr with a non-zero exit.
That is strictly more information than the `.expect("invalid configuration")` it
replaced, because `envconfig::Error` names the offending variable — but it is not
a tracing event, and the dotenv outcome is lost on that path too. Accepted
knowingly; the alternative was the rejected option above.

**Consequence: the `murmur-common` audit (2026-09-02) is closed.** `telemetry`
now has two real callers and has earned its place. `jetstream` has one today —
only `ingest` parses — which by the audit rule would send it back; it stays
because slice 6 has the consumer parsing into `JetstreamMessage` for the
ClickHouse columns, and that is a decided design rather than a hope. `common`
gains a `dotenvy` dependency, which every future crate depending on it inherits.
`ingest` lost `dotenvy` and `tracing-subscriber`, the consumer lost `dotenvy` and
`serde_json`; only `common` constructs a subscriber now.

**Consequence: runtime log-level reload stays deferred, with a reason.** The
added requirement in `docs/tasks/done/tracing-logging.md` was to change the
level without a restart. Confirmed unnecessary for now: an env var plus a restart
is acceptable. Also confirmed that PostHog does not do it — zero occurrences of
`tracing_subscriber::reload` or signal handling across all 98 crates; their
`debug_or_info!` switches on an HTTP request header, per request, which a stream
has nothing to attach to. The one component where it genuinely matters is
`ingest`, because a restart resubscribes from "now" with the cursor unpersisted,
and that is `docs/tasks/high/cursor-resume.md`.

**Revisit when** a third binary appears and the per-binary `impl` starts to feel
like tax, or when `ingest` needs `init` to return a reload handle and the
consumer does not — the first concrete divergence between the two boots, which is
the thing that would justify splitting them again.

---

## 2026-10-26 — The archive is written before the parse, and is a superset of the topic

**Context.** `handle_message` parsed each Jetstream message with
`serde_json::from_str(&raw).unwrap()` and only then wrote the raw bytes to the
archive. The payload comes from a third party whose lexicons evolve and where
anyone can invent a record type, so the parse can fail on input no local change
can prevent. With no supervisor, a panic left the process dead; with the cursor
unpersisted, the restart gap was unrecoverable.

**Decision.** Log `error!` and skip the event instead of panicking, and move the
archive write **above** the parse. An unparseable event is archived but not
produced to Kafka.

**Why the reordering matters more than the panic.** The old order guaranteed that
the one event you could not understand was also the one event you did not keep —
destroying the only evidence of what the new record type looks like. The archive
is the durable sink; this makes it durable for precisely the cases worth
investigating.

**Consequence: the archive and the topic are no longer the same set.** The
produce needs `parsed.payload.uri()` for the key, so a message that fails to
parse cannot be produced. The archive is a superset. Anything later reconciling
the two — a replay, a count, a completeness check — has to know that.

**Revisit when** a malformed event is actually seen in the wild. The `error!`
line is the trigger: it means a record type exists that `JetstreamMessage` does
not model, and the archived bytes are the specification for fixing it.

---

## 2026-10-26 — Consumer group, offset reset and commit strategy

**Context.** Slice 5 required a consumer and left four choices to be made
deliberately rather than by default. Recorded here because the code answers them
and nothing else did.

**Decision — group id from config, with a default.** `GROUP_ID`, defaulting to
`murmur-ingestion-consumer`. The group id is the identity that makes resuming
work, so a silent change resets the consumer's position; a default in one place
beats the same string in a `.env` on every machine.

**Decision — `auto.offset.reset=earliest`.** Only consulted the first time a
group ever reads, and inert afterwards. Earliest means a fresh group replays the
retained history rather than silently starting at "now", because a gap is worse
than a duplicate when ClickHouse deduplicates on `(jetstream_event_at, uri)`
anyway.

**Decision — automatic commit on a timer, plus an explicit
`commit_message`.** Two mechanisms for one job, and `enable.auto.offset.store`
defaults on, so an offset is marked committable the moment `recv()` hands the
message over — before the code has looked at it. Harmless while "handled" means
"printed". **Not harmless in slice 6**, where handled means written to
ClickHouse: as configured, a crash between receive and insert loses those
messages permanently. `posthog/rust/kafka-deduplicator/src/kafka/config.rs:25-41`
sets both to false, which is the shape that makes the choice explicit.

**Decision — a malformed payload is logged and skipped, not fatal.** A consumer
that panics on one bad message can never make progress past it. A null payload
and a non-UTF-8 payload both degrade to an empty string and a `warn!`.

**Verified 2026-10-26**, not assumed. Across two restarts the consumer resumed
at the exact next offset — last printed 18592, nothing printed when caught up,
first printed 18593 after new events arrived — and the events read back matched
the producer's `seq` values and text in order, including a `delete` with no
`record` and no `cid`.

**Revisit when** slice 6 lands. The commit strategy is the one that becomes
consequential, and it should be decided there rather than inherited from the
rdkafka example.
