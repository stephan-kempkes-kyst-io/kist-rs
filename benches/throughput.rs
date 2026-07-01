use criterion::{
    black_box, criterion_group, criterion_main,
    BenchmarkId, Criterion, Throughput,
};
use kist::{Config, Queue};
use tempfile::tempdir;

fn bench_push(c: &mut Criterion) {
    let payload_sizes = [64usize, 256, 1024, 4096];

    let mut group = c.benchmark_group("push");

    for size in &payload_sizes {
        let payload = vec![0xABu8; *size];

        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            size,
            |b, _| {
                let dir = tempdir().unwrap();
                let config = Config::new(dir.path().to_path_buf())
                    .max_queue_size(1024 * 1024 * 1024); // 1GB
                let mut queue = Queue::open(config).unwrap();

                b.iter(|| {
                    queue.push(black_box(&payload)).unwrap();
                });
            },
        );
    }

    group.finish();
}

fn bench_push_batch(c: &mut Criterion) {
    let batch_sizes = [10usize, 100, 1000];
    let payload = vec![0xABu8; 256];

    let mut group = c.benchmark_group("push_batch");

    for batch_size in &batch_sizes {
        group.throughput(Throughput::Elements(*batch_size as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            batch_size,
            |b, &size| {
                let dir = tempdir().unwrap();
                let config = Config::new(dir.path().to_path_buf())
                    .max_queue_size(1024 * 1024 * 1024);
                let mut queue = Queue::open(config).unwrap();

                b.iter(|| {
                    for _ in 0..size {
                        queue.push(black_box(&payload)).unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_peek_commit(c: &mut Criterion) {
    let mut group = c.benchmark_group("peek_commit");
    let payload = vec![0xABu8; 256];

    group.bench_function("256b", |b| {
        let dir = tempdir().unwrap();
        let config = Config::new(dir.path().to_path_buf())
            .max_queue_size(1024 * 1024 * 1024);
        let mut queue = Queue::open(config).unwrap();

        // Pre-fill with records
        for _ in 0..10_000 {
            queue.push(&payload).unwrap();
        }

        b.iter(|| {
            if let Some(record) = queue.peek().unwrap() {
                black_box(record.as_bytes());
                queue.commit().unwrap();
            } else {
                // Refill when empty
                for _ in 0..1000 {
                    queue.push(&payload).unwrap();
                }
            }
        });
    });

    group.finish();
}

fn bench_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("roundtrip");

    let payload_sizes = [64usize, 256, 1024];

    for size in &payload_sizes {
        let payload = vec![0xABu8; *size];

        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            size,
            |b, _| {
                let dir = tempdir().unwrap();
                let config = Config::new(dir.path().to_path_buf())
                    .max_queue_size(1024 * 1024 * 1024);
                let mut queue = Queue::open(config).unwrap();

                b.iter(|| {
                    queue.push(black_box(&payload)).unwrap();
                    let record = queue.peek().unwrap().unwrap();
                    black_box(record.as_bytes());
                    queue.commit().unwrap();
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_push,
    bench_push_batch,
    bench_peek_commit,
    bench_roundtrip
);

criterion_main!(benches);
