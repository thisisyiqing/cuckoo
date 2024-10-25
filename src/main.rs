mod cuckoo;
use cuckoo::CuckooHashTable;
use bincode;
use hex;

fn main() {
    // let mut serialized = bincode::serialize("abc").unwrap();
    // println!("{}", serialized.len());
    // println!("{}", hex::encode(serialized));

    // serialized = bincode::serialize(&15).unwrap();
    // println!("{}", serialized.len());
    // println!("{}", hex::encode(serialized));

    let mut hash_table = CuckooHashTable::new();

    // Insert some entries
    hash_table.insert("key1", "value1").unwrap();
    hash_table.insert("key2", "value2").unwrap();
    hash_table.insert("key3", "value3").unwrap();
    hash_table.insert("key4", "value4").unwrap();
    hash_table.insert("key5", "value5").unwrap();
    hash_table.print_all();
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
    println!("Lookup key4: {:?}", hash_table.lookup(&"key4"));
    hash_table.print_all();

    // Remove
    hash_table.remove(&"key2");
    println!("Lookup key2 after removal: {:?}", hash_table.lookup(&"key2"));
    println!("Lookup key1: {:?}", hash_table.lookup(&"key1"));
    println!("Lookup key3: {:?}", hash_table.lookup(&"key3"));
    hash_table.print_all();

    hash_table.insert("key2", "value2new").unwrap();
    println!("Lookup key2: {:?}", hash_table.lookup(&"key2"));
}
