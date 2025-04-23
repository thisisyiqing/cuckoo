use criterion::{criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use cuckoo::CuckooHashTable;

fn bench_concurrent_insert(c: &mut Criterion) {
    const NUM_THREADS: usize = 8;
    const INSERTS_PER_THREAD: usize = 10_000;

    let mut group = c.benchmark_group("cuckoo");

    group
        .measurement_time(Duration::from_secs(60))
        .sample_size(50);

    group.bench_function("concurrent_insert", |b| {
        b.iter(|| {
            let table = Arc::new(CuckooHashTable::new());
            let mut handles = vec![];

            for thread_id in 0..NUM_THREADS {
                let table_clone = Arc::clone(&table);
                let handle = thread::spawn(move || {
                    for i in 0..INSERTS_PER_THREAD {
                        let key = thread_id * INSERTS_PER_THREAD + i;
                        let value = format!("val-{}", key);
                        table_clone.insert(key, value);
                    }
                });
                handles.push(handle);
            }

            for handle in handles {
                handle.join().unwrap();
            }

            assert_eq!(table.get_vec().len(), NUM_THREADS * INSERTS_PER_THREAD);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_concurrent_insert);
criterion_main!(benches);
