# Task — a consumer reads `murmur.events`

**Status: TODO** (opened 2026-10-14)

Slice 5 of `docs/tasks/pipeline-thin-slice.md`.

---

## How this fits the project, in plain terms

murmur watches everything posted to Bluesky as it happens, so that later it can
answer questions about a slice of it — how many posts mentioned something in the
last hour, which accounts are unusually busy, what is trending in one corner of
the network.

Right now the system can **listen and remember**. It holds a connection to
Bluesky's firehose, writes every event to a file on disk, and hands every event
to a queue. What it cannot do is **pick anything back up**. The queue is a
letterbox nobody opens.

This slice builds the first reader: a second program whose only job is to take
events out of the queue. That matters for three reasons.

**It completes the shape of the system.** Everything so far captures; nothing
consumes. Once a reader exists, the remaining two slices are small — the reader
writes to a database, and then a query asks it something true. The pipeline the
whole project is named after starts existing here.

**It is why there is a queue at all.** Two programs that hand work to each
other directly must run at the same speed and be up at the same time. A queue
between them removes that: the reader can be restarted, be slow, or be switched
off for a day, and the writer neither knows nor cares. The events wait. This
slice is the first time that property is actually used rather than asserted.

**It is where the hardest idea in streaming systems becomes concrete.** When a
reader stops and starts again, how does it know where it got to? Not at the
beginning, or it reprocesses everything. Not at the end, or it skips whatever
arrived while it was down. The queue remembers a position *for that reader*, and
getting that right — and knowing what happens when it goes wrong — is most of
what operating a system like this consists of. It is also the same question
`ingest` has on the Bluesky side and has not yet answered, which is tracked
separately in `cursor-resume.md`.

Keep it to consuming and printing. Nothing is stored, nothing is queried. The
point is to prove that events come out, and that stopping and starting does not
lose or repeat them.

---

## What exists now

`ingest` produces every Jetstream event to `murmur.events`, keyed by the
record's `at://` URI, with the raw Jetstream JSON as the payload — see
`DECISIONS.md` and slice 6 of the story for why that shape. Verified on
2026-10-02: offsets advance one per event, and the payload comes back
unmodified.

Nothing reads the topic except `kafka-console-consumer` run by hand. The
workspace has two crates, `common` and `ingest`, and one binary.

## What that prevents

**Nothing downstream of Kafka exists.** ClickHouse has a table and one
hand-inserted row; slice 6 needs a program to fill it and slice 7 needs
something to query. Both wait on this.

**The queue's value is unproven.** The argument for Kafka over a direct write
is that producer and consumer are decoupled. With no consumer, that is a claim.

**Offsets and consumer groups are unexercised.** They are the mechanism the rest
of the project leans on — phase 3 measures consumer lag, phase 4 observes
partition skew — and neither can be measured against something that does not
exist.

## Why now

Slice 4 landed on 2026-10-02 and the producer works. This is the only thing
between the current state and a pipeline that runs end to end, which is the
definition of phase 2 being done.

## Done when

```bash
# produce something to read: the topic is empty, see Gotchas
cargo run -p ingest          # ~1 minute, then stop

cargo run -p <consumer>      # prints events

# then the real test
# stop the consumer, run ingest again, start the consumer again
```

Two observable claims:

1. It prints events the producer wrote — same `at://` keys, same payloads.
2. **Restarting it resumes where it stopped**, rather than from the beginning or
   from now. Events produced while it was down are printed when it comes back,
   exactly once, and events it already printed are not printed again.

The second is the one worth proving deliberately, because the failure modes look
like success: a consumer that silently restarts from the end looks fine until
you notice a gap, and one that restarts from the beginning looks fine until the
topic is large.

## The new material

Four ideas, none of which have appeared in this project yet:

- **Consumer group** — a name. Kafka stores a position per group per partition,
  so the position belongs to the group, not the process. Restarting with the
  same group id resumes; a different group id starts over.
- **`auto.offset.reset`** — what to do when a group has *no* stored position:
  start at the earliest retained message, or at the latest. This only applies to
  a group's first ever read, which is why it is so often misunderstood.
- **Committing** — telling Kafka "I have handled everything up to here". Either
  the client does it on a timer, or the code does it explicitly. The choice
  decides whether a crash loses work or repeats it.
- **Rebalancing** — when group membership changes, partitions are reassigned.
  Irrelevant with one consumer and one partition, and the reason consumer code
  looks more complicated than it needs to in every real example you will read.

## Decisions you own

- **Where the consumer lives.** A third crate in the workspace is the expected
  answer, per `DECISIONS.md` 2026-09-28: split processes where a queue already
  separates them. Its name is yours.
- **The group id**, and whether it comes from config or is fixed in code. It is
  the identity that makes resuming work, so losing or changing it silently
  resets the consumer's position.
- **`auto.offset.reset`.** Earliest means a fresh group replays the retained
  history; latest means it only sees new events. They differ sharply the first
  time you run it and are identical afterwards.
- **Commit strategy.** Automatic on a timer is one line; explicit after
  handling each message is slower and precise. Slice 6 makes this consequential,
  because then "handled" means written to ClickHouse — commit before writing and
  a crash loses a batch, commit after and it writes twice. Deciding now is fine;
  deciding deliberately matters.
- **What a malformed payload does.** `ingest` calls `.unwrap()` and would panic.
  A consumer that panics on one bad message cannot make progress past it, ever.
  This is the first place in the project where that is unacceptable rather than
  untidy.

## Pointers

| What | Where |
|---|---|
| Minimal consumer, start here | `docs.rs/crate/rdkafka/0.39.0/source/examples/simple_consumer.rs` |
| Offsets and commit ordering, in code | same folder, `at_least_once.rs` |
| Consumer config with real-world defaults | `posthog/rust/kafka-deduplicator/src/kafka/config.rs:25-41` — sets `enable.auto.commit=false`, `enable.auto.offset.store=false`, session and heartbeat timeouts |
| Explicit commit | `posthog/rust/kafka-deduplicator/src/kafka/batch_consumer.rs:312` — `consumer.commit(&list, CommitMode::Sync)` |
| A real consumer loop, including a message that fails to deserialise | same file, `:356-390` |
| The shared types to parse into | `murmur_common::jetstream::JetstreamMessage` |

## Two triggers this fires

Both were deliberately pinned to "when a second binary exists", which is now:

- **The `murmur-common` audit** (`DECISIONS.md` 2026-09-02). Check every item in
  that crate: if the consumer does not use it, it was `ingest`-specific and
  should move back. Currently `jetstream` (the consumer needs it) and
  `telemetry` (nothing declares it).
- **Telemetry** (`docs/tasks/medium/tracing-logging.md`). The reasoning for
  deferring was that a second binary should be *born* with it while `ingest` is
  converted alongside, rather than converting `ingest` twice. Deferring again
  costs exactly that.

Taking all three at once is a large sitting. Deferring telemetry a second time
is a decision, not a non-decision.

## Gotchas, as of 2026-10-14

- **The topic is empty.** `earliest` and `latest` are both 14925: 14,925
  messages were produced, and all of them aged out at the broker's default
  7-day retention. Run `ingest` for a minute before expecting the consumer to
  print anything.
- **That also makes the resume test subtle.** A fresh group with
  `auto.offset.reset=earliest` reads nothing, because there is nothing before
  14925 — which looks identical to a broken consumer. Produce first.
- **Meanwhile 17,413 events are still on disk**, including a 14 MB file from
  2 October. Kafka forgot them; the archive did not. That is the two-sinks
  decision paying off, and worth looking at once.
- **The payload is raw Jetstream JSON**, so parse it with
  `murmur_common::jetstream::JetstreamMessage` — not a consumer-local struct.
  One definition of the protocol, shared.
- **ClickHouse is stopped.** `docker compose up -d` brings it back; not needed
  until slice 6.

## Out of scope

ClickHouse inserts, batching, dead-letter handling, metrics, more than one
partition, rebalancing behaviour. Slices 6 and 7, phases 3 and 4.
