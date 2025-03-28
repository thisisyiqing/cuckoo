#![warn(clippy::all)]
use cuckoo::{CuckooHashTable, KeyVal};
use loom::sync::Arc;
use loom::thread;
use rand::{Rng, SeedableRng};

#[test]
fn test1() {
    loom::model(test1_body);
}

fn test1_body() {
    const THREADS: u32 = 2;

    let ht = Arc::new(CuckooHashTable::new());

    let mut handles = Vec::new();

    fn thread_func(ht: Arc<CuckooHashTable<u32, u32>>, data: Vec<KeyVal<u32, u32>>) {
        for entry in data {
            ht.insert(entry.key, entry.value);
        }
    }

    let mut rng = rand::rngs::StdRng::seed_from_u64(10);
    let mut rand_nums: Vec<KeyVal<u32, u32>> = (0..3)
        .map(|_| KeyVal {
            key: rng.random(),
            value: rng.random(),
        })
        .collect();

    for _ in 0..THREADS {
        let ht_clone = ht.clone();
        let rand_nums = rand_nums.clone();
        let handle = thread::spawn(move || thread_func(ht_clone, rand_nums));
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let mut elems = ht.get_vec();
    elems.sort_by_key(|kv| kv.key);
    rand_nums.sort_by_key(|kv| kv.key);
    assert_eq!(elems, rand_nums);
}
