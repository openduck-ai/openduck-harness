---
title: Jemalloc Heap Profiles
sidebar_label: Jemalloc Heap Profiles
sidebar_position: 67
description: Build openduck with jemalloc sampling and dump a heap profile from openduck serve with SIGUSR2.
---

Use a jemalloc sampling profile when `openduck serve` holds more memory than the work in progress explains. The profile names the stacks that still own sampled bytes.

Two Cargo features on `openduck-cli` control this:

| Feature | Default | Effect |
| --- | --- | --- |
| `jemalloc` | yes, except MSVC | jemalloc is the global allocator. Background threads return dirty pages. `malloc` is overridden, so Rust and C allocations share one heap. |
| `jemalloc-prof` | no | Turns sampling on, links libunwind, and makes `openduck serve` write a heap profile on `SIGUSR2`. |

A normal `cargo build` already links jemalloc. It does not write profiles. `goose` and `duck` are the same program as `openduck`, so `goose serve` and `duck serve` follow the same steps. MSVC builds omit both features.

## Build the profiling binary

`jemalloc-prof` needs libunwind headers and `pkg-config` at compile time. On Debian and Ubuntu:

```bash
sudo apt install libunwind-dev pkg-config
```

From the repository root:

```bash
source bin/activate-hermit
cargo build -p openduck-cli --bin openduck --features jemalloc-prof
```

The binary is `target/debug/openduck`. For a release binary, add `--release` and use `target/release/openduck` in every later command. `jeprof` must see the same file you started.

Stop the running server and start it again from that binary, with the same arguments and environment you already use. `openduck serve` still requires `OPENDUCK_SERVER__SECRET_KEY` unless you pass `--dangerously-unauthenticated`. A process started from an older binary keeps that older allocator for its whole life.

## Confirm profiling is active

`openduck serve` writes these records to stderr and to the server log. On Linux that file is:

```text
~/.local/state/openduck/logs/server/<YYYY-MM-DD>/<YYYYMMDD_HHMMSS>-serve.log
```

`OPENDUCK_PATH_ROOT` moves this under `<root>/state/logs/server/`. A machine that still has only the legacy Goose state directory uses `~/.local/state/goose/logs/server/` until the OpenDuck directory exists.

A profiling server logs this at startup:

```text
jemalloc profiling is active; SIGUSR2 writes a heap profile
```

The same record includes `conf`:

```text
background_thread:true,prof:true,prof_active:true,lg_prof_sample:19,prof_prefix:/tmp/od-heap
```

In the JSON log file the message is the `fields.message` value. A default build logs `jemalloc is the allocator for openduck serve` and has no dump handler.

The listener is installed only for the `serve` command. `SIGUSR2` terminates a Unix process that has no handler, so send it only to a `serve` process that logged the profiling line. If the handler cannot be installed, the log says `failed to listen for SIGUSR2`.

On a non-Unix, non-MSVC build the process logs `jemalloc profiling is compiled in` and does not listen for a signal.

## Dump a profile

Find the server pid, then signal it:

```bash
pgrep -af 'openduck serve'
kill -USR2 <pid>
```

Each signal writes a new file. The name is `{prefix}.{pid}.{unix_secs}.heap`. The default prefix is `/tmp/od-heap`:

```text
/tmp/od-heap.497427.1792411591.heap
```

```bash
ls -lt /tmp/od-heap.<pid>.*.heap | head
```

A successful dump logs `wrote jemalloc heap profile` with three fields:

| Field | Meaning |
| --- | --- |
| `path` | File just written |
| `allocated` | Bytes jemalloc still counts as in use |
| `resident` | Bytes jemalloc counts as resident |

Both counts are in bytes, refreshed just before the log line. A failed dump logs `jemalloc prof.dump failed` with `error` and `path`. A prefix that contains an interior NUL logs `jemalloc profile path contains an interior nul` and skips the write.

The first line of the file looks like `heap_v2/524288`. `524288` is the sample interval in bytes: one sample per 512 KiB of allocation on average (`lg_prof_sample:19`). The body is sampled live objects and their stacks. Bytes the application has already freed stay out of the file, including pages jemalloc is still holding. Compare `resident` with `allocated`. A large gap is retained free memory, and it will not appear as stacks in the profile.

C allocations are in the profile. The profiling build overrides `malloc`.

## Change where dumps go

`OPENDUCK_JEMALLOC_PROF_PREFIX` is the prefix `openduck serve` passes to each `SIGUSR2` dump. There is no `GOOSE_*` alias. The directory must already exist. The rest of the filename stays `.{pid}.{unix_secs}.heap`.

```bash
OPENDUCK_JEMALLOC_PROF_PREFIX=/var/tmp/od-heap openduck serve --host 127.0.0.1 --port 3000
```

That variable applies only to the signal dump. `prof_prefix:/tmp/od-heap` in the built-in options is a separate jemalloc setting, used when jemalloc writes a dump on its own.

## Change the sample rate

With `MALLOC_CONF` unset, jemalloc reads this compiled string:

```text
background_thread:true,prof:true,prof_active:true,lg_prof_sample:19,prof_prefix:/tmp/od-heap
```

Set `MALLOC_CONF` before the process starts when you want different options. The variable replaces the whole string, so repeat every option you still want. `prof:true` and `prof_active:true` keep sampling on.

`lg_prof_sample` is the log base 2 of the average number of bytes between samples. `19` is 512 KiB. `17` is 128 KiB. A smaller interval makes a heavier, finer profile.

```bash
export MALLOC_CONF="background_thread:true,prof:true,prof_active:true,lg_prof_sample:17,prof_prefix:/tmp/od-heap"
```

`background_thread:true` lets jemalloc return dirty pages from a background thread. This build leaves `lg_prof_interval` unset, so jemalloc does not write interval dumps by itself. Add `lg_prof_interval` inside `MALLOC_CONF` when you want those files. They use `prof_prefix`. `SIGUSR2` still uses `OPENDUCK_JEMALLOC_PROF_PREFIX`, or `/tmp/od-heap` when that variable is unset.

## Read the profile

On Debian and Ubuntu, `jeprof` comes from `libjemalloc-dev`:

```bash
sudo apt install libjemalloc-dev
jeprof --text ./target/debug/openduck /tmp/od-heap.<pid>.<secs>.heap
```

Pass the binary that produced the profile. A rebuild moves function addresses, and `jeprof` then attributes samples to the wrong frames.

`jeprof --text` prints a table. For each function:

| Column | Meaning |
| --- | --- |
| flat | Bytes sampled in that function itself |
| flat% | Flat bytes as a share of the profile |
| sum% | Running total of flat% down the list |
| cum | Bytes sampled in that function and everything it called |
| cum% | Cum bytes as a share of the profile |

Start with the highest `cum` entries. Those are the stacks that still hold sampled memory.

An SVG call graph needs Graphviz (`dot`):

```bash
sudo apt install graphviz
jeprof --svg ./target/debug/openduck /tmp/od-heap.<pid>.<secs>.heap > /tmp/od-heap.svg
```

`jeprof` symbolizes with `llvm-symbolizer` or `addr2line` when one is on `PATH`. If Rust names stay mangled, pipe the text through `rustfilt`.

## What to do with the result

Take one dump while the server is idle, then another after the action you suspect, and diff the top cumulative stacks. The idle dump is the retained working set. The second dump shows what that action kept alive.

The profile answers "which live allocations remain." It does not list memory already returned to jemalloc. Use the `allocated` and `resident` fields on the dump log for that split. Profiles also describe only the process you signaled. An older server that was not built with `jemalloc-prof` has no samples to dump.

Source for this behavior:

- `crates/openduck-cli/Cargo.toml` features `jemalloc` and `jemalloc-prof`
- `crates/openduck-cli/src/main.rs` installs the allocator and the `malloc_conf` symbol
- `crates/openduck-cli/src/jemalloc_prof.rs` listens for `SIGUSR2` and writes the file
- `crates/openduck-cli/src/cli.rs` calls that listener only for `serve`
