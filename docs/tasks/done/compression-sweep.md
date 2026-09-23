# Task — compress finished archive files with a sweep

**Status: DONE** (2026-09-18). Kept for the reasoning, not the checklist.

| Slice | State |
|---|---|
| 1 — full path in `Archive` | done |
| 2 — compress one file (compress, sync, verify, rename, delete) | done, verified with the zstd CLI |
| 3 — the sweep | **cut** — retroactive compression not wanted |
| 4 — wire into rotation, in the background | wiring done; `spawn_blocking` deferred behind a measured trigger, see `DECISIONS.md` 2026-09-17 |
| 5 — step through it in a debugger | done |

Measured on the way: 3.61x on a fixed 864,619-byte archive, 3.86-3.88x on two
live rotations, all passing `zstd -t` with `Check: XXH64` present and no
`.tmp` left behind.

Replaces sub-tasks 4 and 5 of `archive-compression.md`. Background reasoning
(batch over streaming, measured numbers) lives there.

**Not the hand-written component.** That is item 5, cursor persistence.

---

## Context

**What exists now.** Rotation finishes the old file and stops
(`rust/ingest/src/main.rs`, rotation block):

```rust
match finished.writer.into_inner() {
    Ok(file) => {
        drop(file);
        //TODO(compress): hand finished.archive_name to the
        //background compression task.
    }
```

`Archive` holds `writer`, `archive_name` (bare filename) and
`latest_file_time`. `create_file` builds the full `path` but throws it away.

**What that prevents.** Every finished `.jsonl` stays uncompressed forever,
and so does anything a previous run left behind (almost every restart leaves
one). There is no point where a file is known to be complete and trustworthy.

**Why now.** The replay corpus needs to be a set of finished, verified files.
Rotation already produces the boundaries; this makes use of them.

---

## Settled

- **Sweep at every rotation.** Not a separate compress-the-finished-file step.
  Right after rotation, the finished file is just one more non-current `.jsonl`,
  so the sweep covers it and any leftovers from earlier runs.
- **Runs in the background** via `spawn_blocking`, so a long compression cannot
  stall the socket into a disconnect while cursor persistence does not exist.
- **Verification:** checksum on when compressing, decompress to the end, compare
  decompressed byte count with the original's size.
- **Naming:** write `X.jsonl.zst.tmp`, rename to `X.jsonl.zst` only after
  verification passes. A `.tmp` is never trusted.
- **`Archive` stores the full path.**

## Sequence for one file

1. Compress `X.jsonl` into `X.jsonl.zst.tmp`, checksum on
2. `sync_all` the temp file
3. Verify (decompress to end, compare byte counts)
4. Rename `.tmp` to `X.jsonl.zst`
5. Remove `X.jsonl`

## Sweep rules

| Found in the directory | Do |
|---|---|
| the live file (the current period's path) | **skip** — always |
| `X.jsonl.zst.tmp` | delete it |
| `X.jsonl` and `X.jsonl.zst` | delete the `.jsonl` (the rename only happens after verification) |
| `X.jsonl` alone | run the sequence above |

---

## Slices — smallest first, each one runs on its own

### Slice 1 — full path in `Archive`

`create_file` already has `path`. Store it; stop depending on `archive_name`
(the flush error message can use `path.display()`).

**Done when:** it compiles and the rotation flush error message still makes
sense.

### Slice 2 — compress one file, synchronously

The five-step sequence as a plain function, no threads, no sweep. Exercise it on
a **copy** of a real file, either from a test or a temporary call at the top of
`main` that you remove afterwards.

**Done when:**

```bash
cp rust/logs_<some>.jsonl /tmp/ztest/
# run your function against /tmp/ztest/logs_<some>.jsonl, then:
ls /tmp/ztest/                                  # only logs_<some>.jsonl.zst, no .tmp, no .jsonl
zstd -t /tmp/ztest/logs_<some>.jsonl.zst        # verifies
zstd -dc /tmp/ztest/logs_<some>.jsonl.zst | wc -l   # same count as wc -l on the original
```

### Slice 3 — the sweep, synchronously

List the directory and apply the rules table, skipping the live file.

**Done when:** a scratch directory prepared with one of each state ends up as
expected:

```bash
mkdir -p /tmp/zsweep && cd /tmp/zsweep
cp <real>.jsonl live.jsonl                   # pass this path as "current" — must be untouched
cp <real>.jsonl orphan.jsonl                 # -> orphan.jsonl.zst
cp <real>.jsonl pair.jsonl && zstd -q pair.jsonl -o pair.jsonl.zst   # -> .jsonl removed
echo junk > stale.jsonl.zst.tmp              # -> deleted
# run the sweep, then:
ls /tmp/zsweep
```

### Slice 4 — wire it into rotation, in the background

Replace `TODO(compress)` with starting a sweep in `spawn_blocking`.

**Done when:** after running across a few minute boundaries, the archive
directory holds exactly one `.jsonl` (the live one), N `.jsonl.zst`, no `.tmp`,
and `zstd -t *.zst` passes. Then `kill -9`, restart, and after the first
rotation the leftover `.jsonl` is compressed too.

### Slice 5 — step through it in a debugger

Learning goal in its own right: prove the feature works by watching it run,
not only by inspecting the files it leaves behind.

Everything needed is already installed: `rust-lldb` (from rustup), plus
CodeLLDB (`vadimcn.vscode-lldb`) and rust-analyzer in VS Code. There is no
`.vscode/` in `murmur/` yet, and none is needed to start.

**Terminal:**

```bash
cd rust
cargo build
rust-lldb target/debug/ingest
```

```
b compress_archive     # or  b main.rs:<line>
run
p path                 # print one variable
fr v                   # every local in this frame
n                      # next line   (s steps in, c continues)
```

`rust-lldb` is lldb plus Rust formatters, so `PathBuf` and `String` print
readably instead of as raw structs.

**VS Code:** rust-analyzer puts a `Run | Debug` codelens above `fn main` and
above any `#[test]`. Debug uses CodeLLDB with no configuration. A `launch.json`
is only needed for preset arguments or environment variables.

**Done when:** you can stop inside the compression path with a breakpoint,
read the source path, the temp path and the byte counts at the moment they are
computed, and step through a rotation from the sweep starting to the original
being deleted.

**Worth knowing where it does not help.** When code silently does nothing —
a `Result` dropped at the call site, a builder created and never used — a
debugger walks you through lines that all "work". Print the error or handle the
`Result` first. Debuggers earn their keep when a value is wrong or control flow
surprises you.

---

## Gotchas, by slice

**Slice 2**
- `Encoder::finish` is required; skipping it leaves a file that will not
  decompress. It hands back the inner writer, which is what you `sync_all`.
- `copy_encode` cannot turn the checksum on, and `copy_decode` returns `()`, not
  the byte count. Write the same few lines yourself; `io::copy` returns the count.
- `with_extension` **replaces** the last extension: `with_extension("zst")` on
  `X.jsonl` gives `X.zst`. Use `"jsonl.zst"` and `"jsonl.zst.tmp"`.
- `rename` silently **overwrites** an existing destination.

**Slice 3**
- The skip-the-live-file rule is what stops a same-period restart from
  compressing the file you are still appending to, then silently overwriting
  that `.zst` at the next rotation.

**Slice 4**
- `spawn_blocking` takes a closure, and it must **own** what it uses: the loop
  keeps mutating `archive` and `config`, so clone the directory and the live
  path before starting. Keep the closure tiny — `move || sweep(dir, live)` —
  with the logic in an ordinary function.
- Two sweeps can overlap if one is still running at the next rotation, and both
  would write the same `.tmp`. Keep the last `JoinHandle`; if
  `is_finished()` is false, skip this rotation's sweep — the next one catches up.

## Reading

| What | Where |
|---|---|
| Template for encode / decode (3 lines each) | `zstd-0.14.0/src/stream/functions.rs:43-56` and `:17-25` |
| Checksum switch | `Encoder::include_checksum`, `zstd-0.14.0/src/stream/mod.rs:86` |
| Checksum is off by default | `zstd-sys-2.0.16+zstd.1.5.7/zstd/lib/zstd.h:462` |
| Decoder validates checksums by default | same header, `:1393` |
| Write → `sync_all` → drop → use by path | PostHog `batch-import-worker/src/source/s3_gzip.rs:148-152` |
| Docs | `cd rust && cargo doc -p zstd --open` |

std / tokio: `io::copy`, `io::sink`, `fs::metadata`, `File::sync_all`,
`fs::read_dir`, `fs::rename`, `fs::remove_file`, `tokio::task::spawn_blocking`,
`JoinHandle::is_finished`.

## Out of scope

- **Power-loss safety.** `sync_all` covers file contents; a rename is only durable
  across power loss if the parent directory is synced too. Process crashes do not
  need it. Decide deliberately later.
- **The four old `logs_*.jsonl` in `rust/`** — outside the archive directory, so
  the sweep never sees them. Move or delete by hand.
- **Switching from minute to hourly rotation.**
- **`tracing`** — separate task, `disposable/tracing-logging.md`.

## When it ships

`docs/DECISIONS.md` is still owed the batch-over-streaming entry (see the
"Still owed" note in the 2026-09-14 entry). The sweep-at-rotation choice and the
temp-name-then-rename rule belong in it too.
