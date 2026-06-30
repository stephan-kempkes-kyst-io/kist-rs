//! Basic usage of Kist.
//!
//! Run with: cargo run --example basic

use kist::{Config, Queue};
use std::path::PathBuf;
use tempfile::tempdir;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use a temporary directory for this example
    let dir = tempdir()?;
    let config = Config::new(PathBuf::from(dir.path()));

    println!("dir: {:?}", dir.path());

    let mut queue = Queue::open(config)?;

    // Push some events
    let events = [
        b"user.login actor=user_123".as_slice(),
        b"data.export actor=user_123 records=15000",
        b"user.logout actor=user_123",
    ];

    for event in &events {
        queue.push(event)?;
        println!("pushed: {}", std::str::from_utf8(event)?);
    }

    println!("queue depth: {}", queue.len());

    // // Read them back in order
    // while let Some(record) = queue.peek()? {
    //     println!("read:   {}", std::str::from_utf8(record.as_bytes())?);
    //     queue.commit()?;
    // }

    // println!("queue empty: {}", queue.is_empty());

    Ok(())
}
