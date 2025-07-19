use criterion::{criterion_group, criterion_main, Criterion, Throughput, BatchSize};
use nyx_fec::{NyxFec, DATA_SHARDS, PARITY_SHARDS, SHARD_SIZE};

fn bench_encode(c: &mut Criterion) {
    let codec = NyxFec::new();
    let mut group = c.benchmark_group("fec_encode");
    // Prepare shards vector for each iteration.
    group.throughput(Throughput::Bytes((DATA_SHARDS * SHARD_SIZE) as u64));
    group.bench_function("encode", |b| {
        b.iter_batched(
            || {
                // Setup: fresh data shards plus parity buffers.
                let mut shards: Vec<Vec<u8>> = (0..DATA_SHARDS)
                    .map(|i| vec![i as u8; SHARD_SIZE])
                    .collect();
                shards.extend((0..PARITY_SHARDS).map(|_| vec![0u8; SHARD_SIZE]));
                let mut mut_refs: Vec<&mut [u8]> =
                    shards.iter_mut().map(|v| v.as_mut_slice()).collect();
                mut_refs
            },
            |mut_refs| {
                codec.encode(&mut_refs).expect("encode");
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

criterion_group!(benches, bench_encode);
criterion_main!(benches); 