# Task — make `docker compose up -d` the only command

**Status: TODO** (opened 2026-09-23) · **Priority: low**

**Blocked by** the two-listener fix — see below. Not startable before it.

---

## What exists now

`docker compose up -d` brings up both services and both report healthy. That is
R1 of `docs/tasks/done/infra-kafka-clickhouse.md`, and it holds — **on this
machine**.

On a machine that has never run murmur, or after the volumes are deleted, two
things are missing and neither is in the Compose file:

- the ClickHouse database and table. Created by hand:
  `docker compose exec -T clickhouse clickhouse-client < clickhouse/schema.sql`
- the Kafka topic `murmur.events`. Created by hand with `kafka-topics --create`

Both commands are written down — the first in `clickhouse/schema.sql`'s header,
the second in `docs/tasks/done/infra-kafka-clickhouse.md`'s Done-when block —
but knowing to run
them, and in what order, is not.

## What that prevents

**The stack is not reproducible in one step.** R1's stated reason was that if
bringing the stack up needs a README of steps, it will be wrong at 3am in three
weeks. Today it needs three steps, two of which are undocumented as a sequence.

**A consumer that checks its topics at startup would fail.** PostHog hit exactly
this and says so in a comment on their own init service: auto-creation fires on a
producer's first write, so a consumer-only service starting first errors on a
missing topic. That arrives with slice 5.

**Deleting a volume becomes a research task** rather than an experiment. Which
matters because deleting a volume is how the persistence tests are run.

## Why this is low priority

Nothing is broken on the current machine — both volumes exist and hold the
schema and the topic. This is insurance against a fresh clone or a deliberately
deleted volume, not a fix for a current failure. It can wait behind anything
that affects data.

## Blocked by: the two-listener fix

A `kafka-init` service runs *inside* the Compose network and would connect to
`kafka:9092`. The broker currently advertises only `PLAINTEXT://localhost:9092`,
so the init container would connect, be told to use `localhost`, resolve that to
itself, and fail.

That is the carried-forward defect in
`docs/tasks/done/infra-kafka-clickhouse.md`, and it is now
the third thing to come due on it, after ClickHouse's Kafka table engine and the
web UI. The fix is described in that task's "advertised listeners" section.

## Done when

```bash
docker compose down
docker volume rm murmur_kafka-data murmur_clickhouse-data
docker compose up -d
# and then, with no further commands:
docker compose exec -T clickhouse clickhouse-client -q "SHOW TABLES FROM murmur"   # events
kafka-topics --bootstrap-server localhost:9092 --list                              # murmur.events
```

## Pointers — PostHog does both halves

| What | Where |
|---|---|
| ClickHouse init SQL mounted into the image's init directory | `posthog/docker-compose.dev.yml:512` |
| A one-shot topic-creating service: `entrypoint: /bin/sh`, `restart: 'no'`, waits for the broker, idempotent | `posthog/docker-compose.dev.yml:551-580` |
| Their reason for pre-creating, in their own comment | same block: auto-creation "only fires on producer-first writes" |

Facts already established:

- The ClickHouse image runs `/docker-entrypoint-initdb.d/*.sql` **only when the
  data directory is empty.** It does nothing to an existing volume, which is
  precisely why this is insurance rather than a fix.
  `CLICKHOUSE_ALWAYS_RUN_INITDB_SCRIPTS` exists if running them on every start is
  wanted instead — harmless while everything is `IF NOT EXISTS`.
- `depends_on` with `condition: service_healthy` is what lets an init service
  wait for readiness instead of sleeping. The healthchecks built in slice 1 are
  what make that possible.

## The other half — what stays prose

A runbook is still wanted for what no script can hold, and it shrinks to about
half a page once the stack provisions itself:

- which port is HTTP (8123) and which is native (9000), and that only one is
  published
- `docker compose exec -T` for feeding stdin or getting one answer, without `-T`
  for the interactive prompt
- host versus inside-the-container, and why `--network=container:` exists
- reading ClickHouse output: "1 row in set" describes the answer, not the table;
  an empty `--list` is success

## Decisions owned

- **Whether to mount the init SQL at all**, given it only ever runs on an empty
  volume.
- **Whether topic creation belongs in Compose or in a script.** Compose keeps one
  source of truth; a script is easier to read and can be run alone.
- **Whether the runbook is a file (`docs/RUNBOOK.md`) or a section of
  `README.md`.** A file keeps the learning frame clean; a section is more
  discoverable.
- **One rule worth adopting either way:** each command lives in exactly one
  place, and other documents link to it rather than repeating it.
