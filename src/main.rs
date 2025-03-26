#![feature(get_mut_unchecked)]
#![warn(clippy::all)]
mod cuckoo;
use cuckoo::CuckooHashTable;
use std::sync::Arc;
use std::thread;

fn main() {
    // let mut serialized = bincode::serialize("abc").unwrap();
    // println!("{}", serialized.len());
    // println!("{}", hex::encode(serialized));

    // serialized = bincode::serialize(&15).unwrap();
    // println!("{}", serialized.len());
    // println!("{}", hex::encode(serialized));

    let hash_table = CuckooHashTable::new();

    // Insert some entries
    hash_table.insert("key1", "value1").unwrap();
    hash_table.insert("key2", "value2").unwrap();
    hash_table.insert("key3", "value3").unwrap();
    hash_table.insert("key4", "value4").unwrap();
    hash_table.insert("key5", "value5").unwrap();
    println!("{hash_table:?}");
    hash_table.insert("key6", "value6").unwrap();
    hash_table.insert("key7", "value7").unwrap();
    hash_table.insert("key8", "value8").unwrap();
    hash_table.insert("key9", "value9").unwrap();
    hash_table.insert("key10", "value10").unwrap();

    // println!("{}", hash_table.hash1(&"key1"));
    // println!("{}", hash_table.hash2(&"key1"));

    // println!("{}", hash_table.hash1(&"key2"));
    // println!("{}", hash_table.hash2(&"key2"));

    // println!("{}", hash_table.hash1(&"key3"));
    // println!("{}", hash_table.hash2(&"key3"));

    // Lookup
    println!("Lookup key1: {:?}", hash_table.lookup(&"key1"));
    println!("Lookup key2: {:?}", hash_table.lookup(&"key2"));
    println!("Lookup key100: {:?}", hash_table.lookup(&"key100"));
    println!("{hash_table:?}");

    // Remove
    hash_table.remove(&"key2");
    println!(
        "Lookup key2 after removal: {:?}",
        hash_table.lookup(&"key2")
    );
    println!("Lookup key1: {:?}", hash_table.lookup(&"key1"));
    println!("Lookup key3: {:?}", hash_table.lookup(&"key3"));
    println!("{hash_table:?}");

    hash_table.insert("key2", "value2new").unwrap();
    println!("Lookup key2: {:?}", hash_table.lookup(&"key2"));

    let hash_table = Arc::new(CuckooHashTable::<&str, &str>::new());

    let entries: &'static [(&'static str, &'static str)] = &[
        ("key1", "value1"),
        ("key2", "value2"),
        ("key3", "value3"),
        ("key4", "value4"),
        ("key5", "value5"),
        ("key6", "value6"),
        ("key7", "value7"),
        ("key8", "value8"),
        ("key9", "value9"),
        ("key10", "value10"),
        ("key11", "value11"),
        ("key12", "value12"),
        ("key13", "value13"),
        ("key14", "value14"),
    ];

    let mut handles = vec![];
    for (key, value) in entries {
        let hash_table = Arc::clone(&hash_table);

        let handle = thread::spawn(move || match hash_table.insert(key, value) {
            Ok(()) => {
                println!("Inserted ({}, {})", key, value);
            }
            Err(e) => {
                println!("Failed to insert ({}, {}): {:?}", key, value, e);
            }
        });
        handles.push(handle);
    }

    // Wait for all insert threads to complete
    for handle in handles {
        handle.join().unwrap();
    }
    println!("{hash_table:?}");
}
