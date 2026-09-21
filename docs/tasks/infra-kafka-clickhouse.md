# Task — a broker and ClickHouse running locally

**Status: TODO** (opened 2026-09-18)

Slice 1 of `docs/tasks/pipeline-thin-slice.md`. Nothing else in that story can
start until this is done.

---

## What you are building

One Compose file that brings up a Kafka-compatible broker and a ClickHouse
server, both reachable **from your machine**, not only from inside Docker.

No Rust and no Node in this slice. That is deliberate: if the first producer
you write fails to connect, you want to already know the broker is fine, so
the only thing left to suspect is your code. Proving the infrastructure
separately is what makes the next slice debuggable.

## Why now

Slices 3 to 7 all depend on it, and `murmur` has no Compose file at all today.
Your `.gitignore` has been anticipating this since the first commit — it
already ignores `clickhouse-data/`, `kafka-data/` and `.volumes/`.

---

## Requirements

**R1 — one command up, one command down.** `docker compose up -d` and
`docker compose down`. If bringing the stack up needs a README of steps, it
will be wrong at 3am in three weeks.

**R2 — reachable from the host.** A client running on your Mac, outside
Docker, must be able to produce and consume. This is the requirement that
fails most often; see the next section.

**R3 — pinned image versions.** No `latest`. Every benchmark you publish has
to be reproducible, and "it was whatever ClickHouse was on Tuesday" is not a
condition you can record in `BENCHMARKS.md`.

**R4 — ClickHouse data survives a restart.** You will write a table in slice 2
and query it in slice 7, possibly days apart. Whether *broker* data survives
is your call — see decisions.

**R5 — "up" means ready.** Both services need healthchecks. Docker reports a
container as running long before Kafka will accept a connection, and a
producer that starts too early fails in a way that looks like a bug in your
code.

**R6 — no auth, no TLS.** Local only, bound to localhost. This is a
deliberate exclusion, not an oversight: SASL and certificates would cost a day
and teach you nothing about ingestion. Write it down as a decision so the
security question reads as answered rather than forgotten.

The acceptance check below is run by hand for this slice. Automating it waits
until Rust can produce, so that one test covers the broker and the producer
together — see `docs/tasks/pipeline-thin-slice.md`, after slice 3.

Q: Feed me the list of services required to start up
A: Two services. That's all. Everything else is out of scope for this slice.

1. Kafka broker. Concepts to look up, in this order:

- Image choice: apache/kafka vs confluentinc/cp-kafka vs bitnami/kafka — they configure differently, pick one and stay in its docs
- KRaft: process.roles, node.id, controller.quorum.voters, cluster ID
- Listeners: listeners, advertised.listeners, listener.security.protocol.map, controller.listener.names
- In a Docker image, all of these appear as KAFKA_-prefixed environment variables

2. ClickHouse server. Much smaller:

- Image clickhouse/clickhouse-server, pinned
- Two ports: HTTP and native — know which is which
- One volume path for the data directory
- Healthcheck via the HTTP ping endpoint

---

## The one thing that will bite you: advertised listeners

Worth understanding before you start, because the symptom does not resemble
the cause.

A Kafka client does not simply talk to the address you gave it. It connects
once to bootstrap, and **the broker replies with the addresses clients should
use from then on**. If the broker advertises its container hostname, a client
on your host will connect successfully, be told to go talk to `kafka:9092`,
fail to resolve that name, and hang or time out — *after* an apparently
successful connection.

So the symptom is "it works from inside the container, and my host client
connects and then dies." You will be tempted to suspect your Rust. Don't.

The fix is two listeners: one advertising an internal name for
container-to-container traffic, one advertising `localhost` on a different
port for you. PostHog's dev stack shows exactly this, and it is worth reading
before you write a line of YAML:

**`posthog/docker-compose.base.yml:209-231`**

```
--kafka-addr internal://0.0.0.0:9092,external://0.0.0.0:19092
--advertise-kafka-addr internal://kafka:9092,external://localhost:19092
```

Two listeners, two ports, two advertised addresses. Your host connects on
19092; anything inside the Compose network uses 9092.

---

## Decisions you own

- **Redpanda or Apache Kafka.** PostHog runs **Redpanda**
  (`redpandadata/redpanda:v25.1.9`) in dev — Kafka-API compatible, one binary,
  no JVM, no Zookeeper. Apache Kafka in KRaft mode is the real thing and is
  what most systems actually run. Redpanda starts faster and has far fewer
  knobs; Kafka is what the ecosystem's documentation, tooling and war stories
  all assume, so everything you read will apply directly. Either is
  defensible, and this one belongs in `DECISIONS.md`.
- **Persistence.** Named volumes or bind mounts? Your `.gitignore` already
  lists `kafka-data/` and `clickhouse-data/`, which suggests bind mounts were
  the original plan. Named volumes are tidier; bind mounts let you look at the
  files.
- **Topic creation.** Auto-create on first produce (PostHog sets
  `auto_create_topics_enabled=true`) or create `murmur.events` explicitly.
  Explicit means you choose the partition count and see it; auto-create means
  one less step and a topic whose settings you never chose.
- **A web UI.** PostHog runs `provectuslabs/kafka-ui`. Useful for seeing
  topics and messages without CLI flags; one more container.

---

## Done when

```bash
docker compose up -d
docker compose ps                    # both healthy, not merely running

# broker, from your host — not from inside the container
<create topic murmur.events>
<produce a message>
<consume it back>

# clickhouse, from your host
curl http://localhost:8123/ping      # expect: Ok.
```

Then the real test of R2 and R4:

```bash
docker compose down && docker compose up -d
```

ClickHouse still answers, and your topic is either still there or you know
exactly why it isn't.

## Pointers

| What | Where |
|---|---|
| Two-listener config, the R2 answer | `posthog/docker-compose.base.yml:209-231` |
| ClickHouse service, image and healthcheck | `posthog/docker-compose.base.yml:193-203` |
| Broker image they use | `redpandadata/redpanda:v25.1.9` |
| ClickHouse image they use | `clickhouse/clickhouse-server:26.6.2.158` |
| Kafka web UI | `provectuslabs/kafka-ui` |

## Out of scope

PostgreSQL, Redis, Prometheus, Grafana, authentication, TLS, more than one
broker, more than one partition. Each has its own slice or its own week.
