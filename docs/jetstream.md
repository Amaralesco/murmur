# Jetstream reference

Protocol facts that are easy to get wrong, verified 2026-08-31 and unchanged
since. Extracted from the old week-by-week plan, which is gone.

## Endpoint

```
wss://jetstream.us-east.bsky.network/xrpc/network.bsky.jetstream.subscribeEvents
wss://jetstream.us-west.bsky.network/xrpc/network.bsky.jetstream.subscribeEvents
```

No authentication for the live tail.

**The gotcha:** the connection requires the WebSocket subprotocol
`xrpc.v1.json`. Without it the handshake fails. In `tokio-tungstenite` that
means building the request and inserting a `Sec-WebSocket-Protocol` header
rather than passing a bare URL.

## Filters

Applied server-side, so a narrow filter means less bandwidth.

- `collections` — NSIDs, repeatable, wildcards like `app.bsky.feed.*`. Max 100.
- `dids` — repeatable. Max 10,000.
- `kinds` — `commit`, `identity`, `account`, `sync`. Omit for all four.

A `collections` filter constrains **commit events only**; identity, account and
sync flow regardless. A commits-only stream needs `kinds=commit` as well.

## Message shape

An envelope with the event under `payload`:

```json
{
  "$type": "message",
  "payload": {
    "$type": "network.bsky.jetstream.subscribeEvents#commit",
    "did": "did:plc:...",
    "seq": 24664288881,
    "time": "2026-08-13T06:47:43.959305Z",
    "operation": "create",
    "collection": "app.bsky.feed.like",
    "rkey": "3msx2efqdxs27",
    "rev": "...",
    "cid": "...",
    "record": { "$type": "app.bsky.feed.like", "...": "..." }
  }
}
```

The record arrives already decoded — no second parse, no CBOR. A `delete`
carries no `record` and no `cid`, only `collection` and `rkey`, so both must be
`Option<T>`.

`seq` is a `u64` (values around 2.4 × 10¹⁰ — `u32` overflows).

## Cursor semantics

- `seq` is monotonically increasing. Reconnect with `?cursor=N`.
- The cursor is **inclusive**: reconnecting at `N` redelivers event `N`.
- Delivery is **at-least-once**. Duplicates across a reconnect are guaranteed,
  not exceptional.
- Replay is bounded by a lookback window, so a long outage loses data.
- Handlers must be idempotent. Key on the record's `at://` URI.

**Consequence for murmur:** duplicates are a normal condition, not a bug to
engineer away at ingest. Do not try to make the cursor exact; make the
downstream idempotent.

## Keepalive, measured 2026-09-17

- The server sends a WebSocket **ping every 30.00s ± 0.07**, on both idle
  connections and ones carrying ~53 events/sec.
- It **closes the connection ~5s after an unanswered ping**.
- `tungstenite` replies with a pong automatically before surfacing the ping
  (`protocol/mod.rs:668-674`), so an empty `Message::Ping` arm is correct.

This is the basis for the read timeout: silence beyond one ping interval is
abnormal, so the stall detector is a multiple of 30s. See `DECISIONS.md`.

## Two couplings to remember

- **`payload: Commit` is only sound while the URL hardcodes `kinds=commit`.**
  A serde enum tagged on `$type` would make other kinds safe.
- **`Record.text` is tied to `collections=app.bsky.feed.post`.** A like has
  `subject`, not `text`.
