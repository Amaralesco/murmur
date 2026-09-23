-- murmur — ClickHouse schema
--
-- Source of truth for the table definition. The copy inside the container's
-- volume is a consequence of this file, not the other way round.
--
-- Apply this file to a running server. -T disables TTY allocation, which is
-- what lets the file arrive on stdin:
--
--   docker compose exec -T clickhouse clickhouse-client < clickhouse/schema.sql
--
-- Read back what the server actually built, to diff against this file.
--
--   docker compose exec -T clickhouse clickhouse-client \
--     -q "SHOW CREATE TABLE murmur.events FORMAT Vertical"
--
-- WARNING — re-running this file is harmless and applies nothing. Everything
-- is IF NOT EXISTS, so once the table exists an edited column here is skipped
-- silently. The file and the server drift apart and nothing says so.
--
-- To actually change the table:
--   * a new or changed column      -> ALTER TABLE, then edit this file to match
--   * the engine, ORDER BY or
--     PARTITION BY                 -> those are fixed at creation. Create a new
--                                     table, INSERT INTO new SELECT FROM old,
--                                     verify counts, RENAME TABLE to swap.
--                                     See schema-notes.md.
--
-- Nothing detects drift automatically. The SHOW CREATE TABLE above is the
-- manual check; automating it is docs/tasks/low/clickhouse-schema-tests.md.
--
-- The reasoning behind every choice here lives in schema-notes.md, in this
-- folder. Change one, change the other.

CREATE DATABASE IF NOT EXISTS murmur;

CREATE TABLE IF NOT EXISTS murmur.events
(
    -- ------------------------------------------------------------------
    -- Identity
    -- ------------------------------------------------------------------

    -- The record's AT Protocol URI, `at://{did}/{collection}/{rkey}`, built
    -- in Rust. Jetstream delivers at-least-once, so this is the key that
    -- decides whether two events are the same record.
    uri                 String,

    -- The account that acted. A DID (decentralised identifier) is the
    -- permanent id of a Bluesky account. Handles like @alice.bsky.social are
    -- renamable aliases; the DID never changes, which is why it and not the
    -- handle is what gets stored.
    did                 String,

    -- The record type, as an NSID — this is the "kind of thing that
    -- happened": app.bsky.feed.post, app.bsky.feed.like,
    -- app.bsky.feed.repost, app.bsky.graph.follow, and so on.
    -- LowCardinality because there are a few dozen values across millions of
    -- rows, so ClickHouse stores an integer per row and a dictionary once.
    -- NOTE: a reply is *also* app.bsky.feed.post. Distinguishing a top-level
    -- post from a comment is inside the record, not in this column.
    collection          LowCardinality(String),

    -- Record key: the id of this record within one account's collection.
    -- Unique per (did, collection), not globally. Bluesky uses TIDs, which
    -- are roughly time-ordered, so it looks random but is not.
    rkey                String,

    -- Jetstream's monotonically increasing sequence number for the event,
    -- and the value used as a cursor to resume the stream. Identical when an
    -- event is redelivered, which is what makes replay possible.
    seq                 UInt64,

    -- Enum8 rather than a string: an unexpected value is rejected at insert
    -- instead of stored. A fourth operation type breaks inserts until this
    -- is ALTERed, which is the intended trade.
    operation           Enum8('create' = 1, 'update' = 2, 'delete' = 3),

    -- Content id of the record. A delete carries no record and no cid.
    cid                 Nullable(String),

    -- The account's repository revision after this change — effectively a git
    -- commit id for one account's repo. Present on create, update and delete
    -- (verified against the archive). Not in the sorting key: it varies in
    -- exactly the same places jetstream_event_at does, one value per commit.
    -- Kept because it is the only field that says two events came from the
    -- same repo commit, which is what the timestamp clustering is.
    rev                 String,

    -- ------------------------------------------------------------------
    -- Three clocks, named for who set them. Most to least trustworthy.
    -- Kept all three so the gaps between them are measurable: that is the
    -- pipeline's latency, and phase 3 alerts on it.
    -- ------------------------------------------------------------------

    -- `payload.time`: when Jetstream emitted the event. Microsecond
    -- precision in the wire format, so DateTime64(6) — milliseconds would
    -- truncate silently.
    jetstream_event_at  DateTime64(6, 'UTC'),

    -- When this row reached us. The only clock we control, and the one that
    -- makes lag measurable: murmur_ingested_at - jetstream_event_at is how
    -- far behind the pipeline is running.
    murmur_ingested_at  DateTime64(3, 'UTC') DEFAULT now64(3),

    -- `record.createdAt`: when the user's client claims the record was
    -- created. Untrusted — wrong clocks, wrong timezones, scheduled posts
    -- and deliberate backdating all produce values that are hours or years
    -- out. Absent on a delete, hence Nullable.
    client_created_at   Nullable(DateTime64(3, 'UTC')),

    -- ------------------------------------------------------------------
    -- Content
    -- ------------------------------------------------------------------

    -- Only ever set for app.bsky.feed.post. A like has no text. This column
    -- is a candidate to move to its own table keyed on `uri`, so that
    -- post-shaped fields stop being null for every other collection —
    -- deferred deliberately, see schema-notes.md.
    text                Nullable(String)
)
ENGINE = ReplacingMergeTree(murmur_ingested_at)
PARTITION BY toDate(jetstream_event_at)
ORDER BY (jetstream_event_at, uri);
