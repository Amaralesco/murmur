# Task — replace `println!` with levelled logging (`tracing`)

**Status: TODO — deferred** (2026-09-18), until `capture` exists.

Not because it stopped mattering. `murmur-common::telemetry` is shared
infrastructure meant for both binaries, and `DECISIONS.md` 2026-09-02 already
makes building `capture` the trigger to review what belongs in `common`.
Doing telemetry then means the new service is born with it and `ingest` gets
converted alongside, rather than converting `ingest` twice.

The cost being accepted: the 24-hour unattended run needs logs you can
diagnose the next morning, so that milestone moves further out.

Source of truth: `docs/WEEK-1.md` (items 1-2, Tuesday), `.kiro/steering/
learning-protocol.md`.

---

## Added requirement — change the level at runtime

Not just a level fixed at boot. The justification is specific to murmur:
restarting to turn logging up currently **loses data**, because reconnect
resubscribes from "now" and the cursor is not persisted. Observing more must
not cost a restart.

**Mechanism, and there is only one.** `tracing_subscriber::reload::Layer`
wraps a filter and returns a `Handle`; `handle.reload(new_filter)` swaps it
while the process runs. Present in tracing-subscriber 0.3.20 and not behind a
feature flag, so it needs no new dependency. The only change it forces is
that `telemetry::init` must **return** the handle instead of swallowing the
filter.

Everything else is about what *triggers* a call to `reload`:

| Trigger | Expressiveness | Cost |
|---|---|---|
| SIGUSR1 / SIGUSR2 | poor — a signal carries no payload, so cycle or toggle only | none; needs the pid |
| SIGHUP + re-read a file | full directive string | a file to own; needs the pid |
| poll a file on a timer | full directive string | a stat every N seconds, N seconds of latency |
| HTTP admin endpoint | full, scriptable via curl | an HTTP server — **but Prometheus needs one anyway**, so possibly one extra route |

**Open questions, to settle before building:**

- One global level, or a full `EnvFilter` directive (`info,events=trace`)?
  The handle can swap an entire filter, so expressiveness costs nothing at
  the mechanism end — only at the trigger end, which is what rules signals
  out.
- Who triggers it: a human at 3am, or a script?
- Ephemeral until restart, or written back to config?
- How near is the Prometheus endpoint? That one answer probably decides
  between the timer and the HTTP route.

**Why PostHog's `debug_or_info!` does not transfer.** Their `chatty_debug_enabled`
is not config at all — it is an HTTP request header
(`capture/src/payload/recordings.rs:58`), so the level varies *per request*
inside one process. That is why a macro is needed there. A murmur config
value is fixed at boot, which is what `LOG_LEVEL` already is, so the macro
would duplicate a level. Per-category control without a macro is a `target:`
on the call site plus a directive in the filter.

---

## Read this first — four things to go over before starting

1. **You are closer than it looks.** `config.rs:7-8` already parses the level
   correctly (`tracing::Level` implements `FromStr`, which is what `envconfig`
   uses), and `common/src/telemetry.rs` already has a working `init()`. What is
   missing is **one line and one call** — `pub mod telemetry;` in
   `common/src/lib.rs`, and calling `init()` from `main`. Until then `LOG_LEVEL`
   is read into config and thrown away.

2. **The `log(Info, "hello")` shape you asked for exists** — it is
   `event!(Level::INFO, "hello")`. But it is a *macro*, and that is the whole
   point: macros capture file, line and module at compile time and skip
   evaluating their arguments when the level is filtered out. A real function
   would format the string even for messages nobody sees, and would report its
   own source location instead of yours. See "tracing is macros, not functions".

3. **Option C is not hypothetical.** PostHog wrote `capture/src/log_util.rs` —
   a macro wrapping the tracing macros — to route a message to `debug` or `info`
   based on a runtime flag. Read it to see what a *justified* wrapper looks
   like, versus one that merely renames `info!`.

4. **The closure blocker is gone.** This work was deferred once over `|_|`
   syntax. The `telemetry.rs` now on disk uses `from_env_lossy()` and contains
   no closure at all.

**The decision likely to be least obvious:** the three per-event calls at
`main.rs:134`, `:136`, `:140`. At 53.7 events/sec that is the difference
between a log you can read the next morning and roughly 4.6 million lines.

---

## Context

**What exists now.** Ten `println!`/`eprintln!` calls in
`rust/ingest/src/main.rs`:

| Line | Call | Notes |
|---|---|---|
| 68 | `eprintln!("connect failed: {e}")` | |
| 77 | `println!("🚨\n🚨\n...")` | |
| 86 | `eprintln!("server closed the connection")` | |
| 94 | `eprintln!("stream error: {e}")` | |
| 98 | `eprintln!("stream ended")` | |
| 134 | `println!("{text}")` | **per event** — entire raw JSON |
| 136 | `println!("\tseq={}", ...)` | **per event** |
| 140 | `println!("\ttext={text}")` | **per event** |
| 144 | `eprintln!("Could not write to disk: {e}")` | |
| 148 | `eprintln!("Could not write to disk: {e}")` | |

Three of the ten fire on **every message** — roughly four lines per event at
53.7 events/sec.

**More already exists than it looks.** Two of the three pieces are built:

- `ingest/src/config.rs:7-8` already parses the level, and it already works —
  `tracing::Level` implements `FromStr`, which is what `envconfig` uses:
  ```rust
  #[envconfig(default = "info")]
  pub log_level: Level,
  ```
- `common/src/telemetry.rs` already contains a working `init(default_level)`
  that builds an `EnvFilter` and installs the subscriber.

**What is missing is one line and one call:**

1. `common/src/lib.rs` does **not** declare `pub mod telemetry;` — the file is
   never compiled, and is untracked in git.
2. `main.rs` never calls `telemetry::init(...)`.

So `LOG_LEVEL` in `rust/.env` is currently read into config and then ignored.

**What that prevents.** No severity, so a disk-write failure and a raw event
dump are indistinguishable. No filtering, so you cannot quiet the per-event
noise without deleting code. No timestamps, so you cannot tell when anything
happened. No module targets, so you cannot say "debug for `ingest`, warn for
everything else".

**Why now.** `docs/WEEK-1.md` requires surviving 24 hours unattended and being
able to diagnose it afterwards. Four lines per event at firehose rate is not a
log you can read the next morning. The archive-compression task also depends on
this: its crash-inspection story assumes you can see *why* the process died.

**Previously blocked by closures.** The deferral was caused by `|_|` syntax in
an example. The `telemetry.rs` now on disk uses `from_env_lossy()` and contains
no closure at all — that blocker is gone.

---

## Options — decide before touching call sites

### The thing to know first: tracing is macros, not functions

You asked for `log(Info, "hello")`. The closest real thing is:

```rust
tracing::event!(Level::INFO, "hello");   // level as an argument
tracing::info!("hello");                 // level in the macro name
```

Both are **macros**, and that is not incidental. They capture file, line and
module at compile time, and when a level is disabled the arguments are never
evaluated. A plain `fn log(level: Level, msg: String)` would format the string
on every call — including the ones that get filtered out — and would report its
own source location instead of yours.

Levels, most to least severe: `error!` `warn!` `info!` `debug!` `trace!`.

### Option A — `common` owns `init()`, services call `tracing::` macros directly

`murmur_common::telemetry::init(config.log_level)` once in `main`, then
`tracing::info!(...)` wherever you log. This is what the existing
`telemetry.rs` was written for, and what every PostHog service does.

- Least machinery; nothing to maintain.
- Call sites use the ecosystem-standard names, so every tracing example on the
  internet applies directly.
- `common` owns only the subscriber setup, which is the part genuinely shared.

### Option B — `common` re-exports the macros

Services `use murmur_common::{info, error};` instead of `use tracing::...`.

- One import path for everything.
- Buys almost nothing, and hides which library you are actually using. Macro
  re-export (`#[macro_export]`, `pub use`) has its own rules and is a detour.

### Option C — your own wrapper macro over tracing

A `murmur_common::log!` that adds project-specific fields or behaviour.

- **PostHog actually does this**: `capture/src/log_util.rs` defines
  `debug_or_info!(flag, field=?value, "message")`, whose doc comment says it
  "accepts the same syntax as tracing macros". It exists to route a message to
  `debug` or `info` depending on a runtime flag — a real need, not decoration.
- Worth doing only once you have that kind of need. Writing a declarative macro
  (`macro_rules!`) to re-implement what `info!` already does is a detour into
  macro syntax that teaches macros, not observability.

### The `common` question this reopens

**PostHog has no shared telemetry crate.** Their `common/` holds 29 crates and
none of them is telemetry; all 12 services build their own subscriber inline in
`main.rs`. The reference implementation is *service-local*.

`docs/DECISIONS.md` 2026-09-02 already chose the opposite deliberately, with
the risk written down and a **review trigger**: when `capture` is built, check
every item in `murmur-common` and move back anything `capture` does not use.
This finding is evidence for that review, not a reason to relitigate now.

---

## Sub-tasks, in order

1. [ ] Add `pub mod telemetry;` to `common/src/lib.rs`. Confirm it compiles.
2. [ ] Call `telemetry::init(config.log_level)` in `main`, immediately after
       `Config::init_from_env()`.
3. [ ] Pick an option above.
4. [ ] Replace the ten call sites, choosing a level for each.
5. [ ] Deal with the three per-event calls (`:134`, `:136`, `:140`) — see
       decisions below.
6. [ ] `git add rust/common/src/telemetry.rs` — currently untracked.
7. [ ] Cargo.toml tidy: if `common` owns `init`, `ingest` needs `tracing` for
       the macros but may no longer need `tracing-subscriber` directly.

## Decisions you own

- [ ] **Which option**, A / B / C.
- [ ] **A level for each of the ten sites.** Worth thinking about what you would
      want to see at 3am with `LOG_LEVEL=warn`, versus while debugging.
- [ ] **The three per-event calls.** Delete, or demote to `trace!`? They are the
      difference between a readable overnight log and 4.6 million lines.
- [ ] **Output format.** PostHog switches on level: human-readable when
      `DEBUG`, JSON otherwise so Loki/Grafana can extract fields
      (`capture/src/main.rs:60-79`). Grafana arrives Thursday.
- [ ] **`LOG_LEVEL` vs `RUST_LOG`.** `telemetry.rs` makes config the default and
      lets `RUST_LOG` override with per-module directives. Confirm that is what
      you want, since it means an env var not in `config.rs` can change
      behaviour.

## Done when

```bash
cd rust
grep -c "println!\|eprintln!" ingest/src/main.rs        # 0

cargo run -p ingest                                      # timestamp + level + target
LOG_LEVEL=warn cargo run -p ingest                       # info lines gone
RUST_LOG=ingest=debug cargo run -p ingest                # debug lines back
RUST_LOG=ingest=debug,tokio=warn cargo run -p ingest     # per-module targeting works
```

Plus: pull the network cable and confirm the reconnect path logs at a level
you would actually have seen overnight.

## Tier-3 pointers (PostHog)

| What | Where |
|---|---|
| Subscriber build, layers, `from_env_lossy` | `capture/src/main.rs:60-79` |
| Debug-vs-production format split (JSON for Loki) | same, ~`:64-85` |
| Custom macro wrapping tracing macros | `capture/src/log_util.rs` |
| Ordinary usage at call sites | `capture/src/ai_endpoint.rs:401,550` |
| Every service builds its own subscriber | 12 `main.rs` files; no shared crate |

## Follow-ups this surfaces

- `serde_json::from_str(&text).unwrap()` (`main.rs:128`) is still the one panic
  on the hot path. Logging the bad payload at `warn!` and continuing is the
  agreed fix, and this task is the prerequisite for it.
- `docs/DECISIONS.md` wants an entry when this ships: which option, why, and the
  PostHog service-local counter-evidence recorded honestly.
