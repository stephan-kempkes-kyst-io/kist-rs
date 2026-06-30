# Kist

A crash-safe, append-only disk queue for Rust.

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)


## What It Is

Kist is a persistent FIFO queue that writes to disk.
A `push` that returns `Ok(())` survives process crashes,
power loss, and OS restarts.

It is designed for the case where you need to buffer events
locally before delivering them to a remote service.


## Why Kist

Most queues are in-memory. If your process dies, you lose
everything buffered. Kist writes to disk first and acknowledges
after fsync.


## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
kist = "0.1"
```

Use it:

```rust
use kist::{Config, Queue};
use std::path::PathBuf;

let config = Config::new(PathBuf::from("/var/lib/myapp/queue"));
let mut queue = Queue::open(config)?;

// Push — durable after this returns
queue.push(b"event payload")?;

// Peek + commit — at-least-once delivery
if let Some(record) = queue.peek()? {
    deliver(record.as_bytes())?;
    queue.commit()?;
}
```


## Guarantees

- **Durability** — every successful push is fsynced to disk
- **Ordering** — records returned in push order (FIFO)  
- **Crash safety** — no corruption after unclean shutdown
- **At-least-once** — records survive until explicitly committed


## Non-Goals

Kist is deliberately narrow:

- No multi-process access
- No network
- No encryption
- No random access


## How It Works

Kist writes records to append-only segment files:

```
queue/
├── 00000000000000000000.seg   ← consumed segments (deleted)
├── 00000000000000001000.seg   ← partially consumed
└── 00000000000000002000.seg   ← active write segment
```

Each record is stored with a length prefix and CRC32C checksum.
On recovery after a crash, Kist scans the active segment and
discards any partially-written records, leaving the queue in a
consistent state.


## Status

Kist is under active development. The API may change before 1.0.


## License

Licensed under either of MIT License or Apache License 2.0
at your option.

Built by [Kyst](https://kyst.io) —
tamper-evident audit logs.
