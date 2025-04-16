#![warn(clippy::all)]
use cuckoo::CuckooHashTable;
use core::hash;
use std::{sync::Arc, thread};

#[test]
fn test_original() {
    // let ht = CuckooHashTable::new();

    // // Insert some entries
    // ht.insert("key1", "value1");
    // ht.insert("key2", "value2");
    // ht.insert("key3", "value3");
    // ht.insert("key4", "value4");
    // ht.insert("key5", "value5");
    // println!("{ht:?}");
    // ht.insert("key6", "value6");
    // ht.insert("key7", "value7");
    // ht.insert("key8", "value8");
    // ht.insert("key9", "value9");
    // ht.insert("key10", "value10");

    // println!("{}", hash_table.hash1(&"key1"));
    // println!("{}", hash_table.hash2(&"key1"));

    // println!("{}", hash_table.hash1(&"key2"));
    // println!("{}", hash_table.hash2(&"key2"));

    // println!("{}", hash_table.hash1(&"key3"));
    // println!("{}", hash_table.hash2(&"key3"));

    // // Lookup
    // println!("Lookup key1: {:?}", ht.lookup(&"key1"));
    // println!("Lookup key2: {:?}", ht.lookup(&"key2"));
    // println!("Lookup key100: {:?}", ht.lookup(&"key100"));
    // println!("{ht:?}");

    // // Remove
    // ht.remove(&"key2");
    // println!("Lookup key2 after removal: {:?}", ht.lookup(&"key2"));
    // println!("Lookup key1: {:?}", ht.lookup(&"key1"));
    // println!("Lookup key3: {:?}", ht.lookup(&"key3"));
    // println!("{ht:?}");

    // ht.insert("key2", "value2new");
    // println!("Lookup key2: {:?}", ht.lookup(&"key2"));

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

        let handle = thread::spawn(move || {
            hash_table.insert(key, value);
        });
        handles.push(handle);
    }

    // Wait for all insert threads to complete
    for handle in handles {
        handle.join().unwrap();
    }
    
    println!("{:?}", hash_table);
    println!("{}", hash_table.get_capacity());
    println!("{}", hash_table.get_vec().len());
}
