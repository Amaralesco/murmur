# Task — verify the database after an image or schema change

**Status: TODO — deferred** (opened 2026-09-23)

Out of scope for slice 2 of `docs/tasks/pipeline-thin-slice.md`, which ends at
one row inserted by hand. Raised because the question came up while building
the schema, and the findings below are worth not rediscovering.

---

## What exists now

`clickhouse/schema.sql` is the source of truth for the table, and
`clickhouse/schema-notes.md` holds the reasoning. The table was created by
running the file against the server. Nothing verifies either of two things:

- that the schema still **behaves** as assumed after the ClickHouse image
  version changes (currently pinned at `clickhouse/clickhouse-server:26.9.1`)
- that the live table still **matches** `schema.sql`

A pure-SQL test doing the first of those was written and deleted the same day,
deliberately: it belongs to this task, not to slice 2.

## What that prevents

**An image bump is currently unverifiable.** The schema leans on four
behaviours that a version change could alter: `Enum8` rejecting unknown values,
`Nullable` staying NULL rather than becoming an empty string, `DateTime64(6)`
preserving microseconds, and `DEFAULT now64(3)` firing on insert. Nothing would
tell you if one changed — the symptom would be wrong data downstream, weeks
later.

**Schema drift is silent.** Editing `schema.sql` and forgetting to apply it
leaves the file and the server disagreeing, with nothing to detect it. The
table is the thing queries run against; the file is the thing under review.

**The engine's contract is assumed.** `ReplacingMergeTree` collapsing a
redelivery but *not* collapsing a create and a later delete of the same record
is the single most important property of this schema, and it is argued in
`schema-notes.md` from measurement rather than enforced anywhere.

## Why not now

Slice 2 ends at one row by hand. Nothing inserts rows programmatically until
slice 6, so a regression has nowhere to come from yet, and the image version
has not changed since the table was created.

The related decision in `docs/tasks/pipeline-thin-slice.md` — automating the
Kafka produce/consume proof after slice 3 — has the same shape, and both may
want the same harness. Worth deciding together rather than twice.

## Done when

There are **two separate jobs** here. Deciding whether both are wanted is part
of the task.

**1. A contract test.** Inserts a known real event into a table with this
schema, asserts it comes back unchanged, and asserts the dedupe behaviour.
Exits non-zero on failure so a script can run it.

```bash
<run it>            # exit 0 on pass, non-zero on failure
```

**2. A drift check.** Compares the live table against `schema.sql`, and fails
when they disagree.

```bash
<run it>            # fails if the server's table is not what the file says
```

## Findings already established, so they are not rediscovered

- **`throwIf(condition, 'message')`** raises a server exception when the
  condition is *true*, which makes a pure-SQL assertion possible. The message
  must be a **constant** — it cannot quote the offending value back, which
  costs real diagnostic value (`Code: 44 ... must be constant`).
- **`clickhouse-client` exits non-zero on a server exception** — observed exit
  `44`. So a `.sql` file of assertions is directly usable from a shell, with no
  output diffing.
- **`CREATE TABLE b AS a` is a faithful clone.** Verified by diffing
  `SHOW CREATE TABLE` of both: columns, types, `DEFAULT`s, engine and its
  version column, `PARTITION BY`, `ORDER BY` and settings all carry over,
  identical. The clone is built at run time, so it cannot drift from the live
  table.
- **But a clone follows the server, not the file.** This is why the two jobs
  above are separate: a test against a clone passes happily while `schema.sql`
  sits unapplied.
- **A drift check cannot be written in SQL**, because SQL cannot read
  `schema.sql`. It also cannot be a plain text diff: the server normalises the
  DDL and strips every comment, so the comparison has to be against a stored
  copy of the normalised form.
- **Dedupe is only visible under `FINAL`.** Insert a duplicate and the row
  count on disk goes up; `SELECT ... FINAL` shows the collapsed view. Any test
  asserting replacement must use it, or it is asserting the opposite.
- **A real event pair to test with** is in
  `rust/logs_2026-09-14T1158.jsonl`: `did:plc:32e2qg7rmbv7yli4wr7qikqx`,
  rkey `3mvi2oabemc2l` — one post created and deleted twenty seconds later, so
  it exercises a populated create, a NULL-heavy delete, and accented text
  containing an apostrophe.

## Decisions owned

- **Which of the two jobs is worth having**, and whether either earns a place
  before something automated writes to the table.
- **Where the tests live.** A `.sql` file of assertions, a shell script, or
  Rust integration tests with `testcontainers` once slice 3 exists. The
  pipeline story already names that third option for Kafka.
- **Clone or dedicated fixture table.** A clone tracks the schema
  automatically; a fixture table written out in full would catch drift but
  repeats the DDL.
- **Whether anything runs these automatically.** There is no CI in this
  project. A test nobody runs after an image bump does not solve the problem it
  was written for.
