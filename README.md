# Kist

A crash-safe, append-only disk queue for Rust.

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

---

## Table of Contents

- [What It Is](#what-it-is)
- [Why Kist](#why-kist)
- [Quick Start](#quick-start)
- [Guarantees](#guarantees)
- [Non-Goals](#non-goals)
- [How It Works](#how-it-works)
- [Configuration](#configuration)
- [Examples](#examples)
- [Development](#development)
  - [Prerequisites](#prerequisites)
  - [Building](#building)
  - [Testing](#testing)
  - [Benchmarking](#benchmarking)
- [Status](#status)
- [Contributing](#contributing)
- [License](#license)

---

## What It Is

Kist is a persistent FIFO queue that writes to disk. A `push` that
returns `Ok(())` survives process crashes, power loss, and OS
restarts. Records are returned by `peek` in the order they were
pushed and are not removed until `commit` is called.

It is designed for the case where you need to buffer events locally
before delivering them to a remote service — and you cannot afford
to lose them if the process dies mid-delivery.

```
Your application
│
│  push(event)          ← returns after fsync
▼
┌─────────┐
│  Kist   │  segment files on disk
│  queue  │  crash-safe, append-only
└─────────┘
│
│  peek() → commit()    ← at-least-once delivery
▼
Remote service
```

---

## Why Kist

Most queues are in-memory. If your process dies, everything
buffered is lost. Kist writes to disk and fsyncs before
acknowledging — nothing is lost regardless of when the process
dies.

| | In-memory queue | Kist |
|--|--|--|
| Survives process crash | ✗ | ✓ |
| Survives power loss | ✗ | ✓ |
| Survives OS restart | ✗ | ✓ |
| At-least-once delivery | ✗ | ✓ |
| Zero external dependencies | ✓ | ✓ |
| Single-process access | ✓ | ✓ |

---

## Quick Start

Add Kist to your `Cargo.toml`:

```toml
[dependencies]
kist = "0.1"
```

Push, peek, and commit:

```rust
use kist::{Config, Queue};
use std::path::PathBuf;

let config = Config::new("/var/lib/myapp/queue");
let mut queue = Queue::open(config)?;

// Push — durable after this returns
queue.push(b"event payload")?;

// Peek — record stays in queue until commit
if let Some(record) = queue.peek()? {
    deliver(record.as_bytes())?;
    queue.commit()?;  // now permanently removed
}
```

---

## Guarantees

**Durability.** Every `push` that returns `Ok(())` is written
to disk and fsynced. The record will survive a process crash,
power loss, or OS restart.

**Ordering.** Records are returned by `peek` in exactly the
order they were pushed. FIFO is strictly maintained across
process restarts.

**Crash safety.** A crash at any point — mid-write, mid-commit,
mid-rotation — leaves the queue in a consistent state. On the
next open, Kist recovers to the last consistent position. A
record that was pushed but whose process crashed before commit
will be returned again on the next `peek`.

**At-least-once delivery.** Records are not removed from the
queue until `commit` is called. If delivery fails or the process
crashes between `peek` and `commit`, the record will be returned
on the next `peek`. Your delivery logic must be idempotent.

**No silent data loss.** Every record is stored with a CRC32C
checksum. `peek` verifies the checksum before returning a record.
A checksum mismatch returns `Err(Error::ChecksumMismatch)` rather
than silently returning corrupt data.

---

## Non-Goals

Kist is deliberately narrow. It does not and will not:

- **Support multi-process access.** The queue is owned by a
  single process. Two processes writing to the same queue
  directory simultaneously will corrupt it.

- **Support network access.** Kist writes to a local filesystem.
  It is not a message broker.

- **Encrypt data at rest.** Payloads are stored as plain bytes.

- **Support random access.** Records are read sequentially.
  There is no seeking to a specific record by index.

- **Replace a message broker.** For multi-producer, multi-consumer,
  or networked use cases, use a message broker. Kist is the
  local buffer before you send to one.

---

## How It Works

Kist writes records to append-only segment files on disk:

```
queue/
├── 00000000000000000000.seg   ← consumed, will be deleted
├── 00000000000000004096.seg   ← partially consumed
└── 00000000000000008192.seg   ← active write segment
```

Total overhead: 12 bytes per record regardless of payload size.

### Read Cursor

The read position (which segment, which byte offset within it) is
persisted to a `read.cursor` file after every `commit`. On reopen,
Kist reads this file to restore the position, so already-committed
records are not returned again.

The cursor file is written atomically via a temp file and rename,
so a crash during a commit leaves the cursor in a consistent state.

### Crash Recovery

On `open`, Kist:

1. Reads `read.cursor` to find the last committed position.
2. Scans the active write segment from the beginning, record by
   record, to find the end of the last fully-written record.
   Any partial write at the tail (from a crash mid-push) is
   silently discarded.
3. Counts unread records from the cursor position using
   header-only reads (reads 12 bytes per record, seeks over
   the payload without reading it).

---

## Configuration

```rust
use kist::{Alignment, Config};

let config = Config::new("/var/lib/myapp/queue")
    // Maximum size of a single segment file.
    // Smaller → more frequent rotation, faster cleanup of
    // consumed data. Larger → fewer filesystem metadata ops.
    // Default: 64 MiB
    .max_segment_size(64 * 1024 * 1024)

    // Maximum total size across all segment files.
    // push() returns Error::QueueFull when this is reached.
    // Default: 512 MiB
    .max_queue_size(512 * 1024 * 1024);
```

---

## Examples

All examples use `tempfile::tempdir()` for storage and clean up
after themselves. Each demonstrates one distinct usage pattern.

### Run an Example

```bash
cargo run --example <name>
```

### Available Examples

#### `basic`
The minimal push/peek/commit cycle. Start here.

```bash
cargo run --example basic
```

---

## Development

### Prerequisites

Install the Rust toolchain via [rustup](https://rustup.rs):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

---

### Building

Build the library in debug mode:

```bash
cargo build
```

Build in release mode (optimised, used for benchmarks):

```bash
cargo build --release
```

Build with all optional features enabled:

```bash
cargo build --all-features
```

Check that the code compiles without producing a binary
(faster than a full build — useful during development):

```bash
cargo check
cargo check --all-features
```

---

### Testing

Run all tests:

```bash
cargo test
```

Run tests with all features enabled:

```bash
cargo test --all-features
```

Run tests in release mode (catches optimisation-dependent bugs):

```bash
cargo test --release
cargo test --all-features --release
```

Run a specific test by name:

```bash
cargo test cursor_persists_across_reopen
```

Run tests in a specific file:

```bash
# Unit tests in src/queue.rs
cargo test --lib queue


Run tests with output visible (useful for debugging):

```bash
cargo test -- --nocapture
```

Show test output even for passing tests:

```bash
cargo test -- --nocapture --show-output
```

List all available tests without running them:

```bash
cargo test -- --list
```

Run tests with a filter — all tests whose name contains "reopen":

```bash
cargo test reopen
```

---

### Benchmarking

Kist uses [Criterion](https://github.com/bheisler/criterion.rs)
for benchmarks. Criterion runs each benchmark multiple times,
computes statistics, and detects performance regressions.

Run all benchmarks:

```bash
cargo bench
```

Save a baseline to compare against later:

```bash
cargo bench -- --save-baseline before-optimisation
```

Compare against a saved baseline:

```bash
cargo bench -- --baseline before-optimisation
```

Generate an HTML report (opens automatically in your browser):

```bash
cargo bench
open target/criterion/report/index.html   # macOS
xdg-open target/criterion/report/index.html  # Linux
```

Compile benchmarks without running them
(useful to verify they build in CI):

```bash
cargo bench --no-run
```

---

## Status

Kist is under active development. The API is not yet stable.

- Minor version bumps before 1.0 may include breaking changes.
- The on-disk format is not yet frozen. A migration path will
  be provided if the format changes.

---

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md)
for the full guide — branching model, commit message format,
code standards, testing requirements, and the release process.

For questions, open a
[GitHub Discussion](https://github.com/kyst/kist/discussions).
For bugs, open a
[GitHub Issue](https://github.com/kyst/kist/issues).
For security vulnerabilities, email security@kyst.io — do not
open a public issue.

---

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

Unless you explicitly state otherwise, any contribution
intentionally submitted for inclusion in Kist by you shall be
dual-licensed as above, without any additional terms or conditions.

---

*Kist is built by [Kyst](https://kyst.io) —
tamper-evident audit logs for SaaS.*
