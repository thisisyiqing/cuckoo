#![warn(clippy::all)]
use cuckoo::{CuckooHashTable, KeyVal};
use loom::sync::Arc;
use loom::thread;
use rand::{Rng, SeedableRng};

#[test]
fn test1() {
    // Test 10 times with different random entries
    for seed in 0..10 {
        loom::model(move || test1_body(seed));
    }
}

fn test1_body(seed: u64) {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let entries: Vec<KeyVal<u32, u32>> = (0..2)
        .map(|_| KeyVal {
            key: rng.random(),
            value: rng.random(),
        })
        .collect();

    let ht = Arc::new(CuckooHashTable::new());

    fn thread_func(ht: Arc<CuckooHashTable<u32, u32>>, entries: Vec<KeyVal<u32, u32>>) {
        for entry in entries {
            ht.insert(entry.key, entry.value);
        }
    }

    let mut handles = Vec::new();
    for _ in 0..2 {
        let ht_clone = ht.clone();
        let entries = entries.to_vec();
        let handle = thread::spawn(move || thread_func(ht_clone, entries));
        handles.push(handle);
    }
    for handle in handles {
        handle.join().unwrap();
    }

    let mut elems = ht.get_vec();
    elems.sort_by_key(|kv| kv.key);

    let mut entries = entries.to_vec();

    entries.sort_by_key(|kv| kv.key);
    assert_eq!(elems, entries);
}
