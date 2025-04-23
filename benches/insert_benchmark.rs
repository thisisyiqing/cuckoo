use criterion::{criterion_group, criterion_main, Criterion};
use rand::{seq::SliceRandom, rng};
use rand::prelude::IndexedRandom;
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

    // group.bench_function("concurrent_insert", |b| {
    //     b.iter(|| {
    //         let table = Arc::new(CuckooHashTable::new());
    //         let mut handles = vec![];

    //         for thread_id in 0..NUM_THREADS {
    //             let table_clone = Arc::clone(&table);
    //             let handle = thread::spawn(move || {
    //                 for i in 0..INSERTS_PER_THREAD {
    //                     let key = thread_id * INSERTS_PER_THREAD + i;
    //                     let value = format!("val-{}", key);
    //                     table_clone.insert(key, value);
    //                 }
    //             });
    //             handles.push(handle);
    //         }

    //         for handle in handles {
    //             handle.join().unwrap();
    //         }

    //         assert_eq!(table.get_vec().len(), NUM_THREADS * INSERTS_PER_THREAD);
    //     });
    // });

    group.bench_function("concurrent_random_lookup", |b| {
        let table = Arc::new(CuckooHashTable::new());
        let mut all_keys = vec![];
    
        // Fill the table and collect all inserted keys
        for thread_id in 0..NUM_THREADS {
            for i in 0..INSERTS_PER_THREAD {
                let key = thread_id * INSERTS_PER_THREAD + i;
                let value = format!("val-{}", key);
                table.insert(key, value);
                all_keys.push(key);
            }
        }
    
        // Shuffle keys and distribute them randomly across threads
        let shuffled_keys = {
            let mut keys = all_keys.clone();
            keys.shuffle(&mut rng());
            Arc::new(keys)
        };
    
        b.iter(|| {
            let mut handles = vec![];
    
            for _ in 0..NUM_THREADS {
                let table_clone = Arc::clone(&table);
                let keys_clone = Arc::clone(&shuffled_keys);
    
                let handle = thread::spawn(move || {
    
                    // Draw random keys for this thread
                    let sample: Vec<_> = keys_clone
                        .choose_multiple(&mut rng(), INSERTS_PER_THREAD)
                        .cloned()
                        .collect();
    
                    for key in sample {
                        let expected = format!("val-{}", key);
                        assert_eq!(table_clone.lookup(&key), Some(expected));
                    }
                });
    
                handles.push(handle);
            }
    
            for handle in handles {
                handle.join().unwrap();
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_concurrent_insert);
criterion_main!(benches);
