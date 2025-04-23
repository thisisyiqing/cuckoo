#![warn(clippy::all)]
use std::fmt::Debug;
use std::hash::{Hash, Hasher};

#[cfg(loom)]
use loom::sync::{Mutex, RwLock};

#[cfg(not(loom))]
use std::sync::{Mutex, RwLock};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyVal<K, V> {
    pub key: K,
    pub value: V,
}

const INIT_SIZE: usize = 400_000;
const MAX_RELOCS: usize = 8;

pub struct CuckooHashTable<K, V> {
    table: RwLock<InnerTable<K, V>>,
}

struct InnerTable<K, V> {
    arr: Mutex<Vec<Option<KeyVal<K, V>>>>,
}

impl<K, V> InnerTable<K, V>
where
    K: Hash + Eq + Clone + Debug,
    V: Clone + Debug,
{
    fn hash1(&self, key: &K, len: usize) -> usize {
        let mut hasher = xxhash_rust::xxh3::Xxh3::with_seed(len as u64);
        key.hash(&mut hasher);
        (hasher.finish() % len as u64) as usize
    }

    fn hash2(&self, key: &K, len: usize) -> usize {
        let mut hasher = xxhash_rust::xxh3::Xxh3::with_seed(len as u64 + 1);
        key.hash(&mut hasher);
        (hasher.finish() % len as u64) as usize
    }

    fn try_direct_insert(&self, new_entry: &KeyVal<K, V>) -> bool {
        let mut arr_guard = self.arr.lock().unwrap();

        let hash1_idx = self.hash1(&new_entry.key, arr_guard.len());
        let hash2_idx = self.hash2(&new_entry.key, arr_guard.len());

        // If this key is already in the table, replace it
        if let Some(entry1) = &arr_guard[hash1_idx] {
            if entry1.key == new_entry.key {
                arr_guard[hash1_idx] = Some(new_entry.clone());
                return true;
            }
        }
        
        if hash1_idx != hash2_idx {
            if let Some(entry2) = &arr_guard[hash2_idx] {
                if entry2.key == new_entry.key {
                    arr_guard[hash2_idx] = Some(new_entry.clone());
                    return true;
                }
            }
        }
        
        // Otherwise, if there's an unused bucket, place our entry there
        if arr_guard[hash1_idx].is_none() {
            arr_guard[hash1_idx] = Some(new_entry.clone());
            return true;
        }
        
        if hash1_idx != hash2_idx && arr_guard[hash2_idx].is_none() {
            arr_guard[hash2_idx] = Some(new_entry.clone());
            return true;
        }
        
        false
    }
    
    fn find_insert_path(&self, key: &K) -> Option<Vec<usize>> {
        if let Some(path) = self.find_path_that_clears_index(key, 1) {
            return Some(path);
        }
        if let Some(path) = self.find_path_that_clears_index(key, 2) {
            return Some(path);
        }
        None
    }

    fn find_path_that_clears_index(&self, key: &K, hash_num: i32) -> Option<Vec<usize>> {
        let arr_guard = self.arr.lock().unwrap();
        let index = if hash_num == 1 {
            self.hash1(key, arr_guard.len())
        } else {
            self.hash2(key, arr_guard.len())
        };

        let mut path = vec![index];
    
        for _ in 0..MAX_RELOCS {
            let prev_idx = *path.last().unwrap();
            
            let Some(entry) = &arr_guard[prev_idx] else {
                // This bucket is empty, so our shift-chain ends!
                return Some(path);
            };
    
            // Get the alternate bucket of this entry.
            let next_idx = if self.hash1(&entry.key, arr_guard.len()) != prev_idx {
                self.hash1(&entry.key, arr_guard.len())
            } else {
                self.hash2(&entry.key, arr_guard.len())
            };
    
            path.push(next_idx);
        }
        None
    }

    fn try_shift_entries(&self, path: &[usize], new_entry: &KeyVal<K, V>) -> bool {
        let mut arr_guard = self.arr.lock().unwrap();
        
        // Shift other entries to free up the first bucket in path
        for i in (0..path.len() - 1).rev() {
            // We can't shift into an occupied bucket
            if arr_guard[path[i + 1]].is_some() {
                return false;
            }
            
            // We can't shift into the wrong bucket
            if let Some(entry) = &arr_guard[path[i]] {
                // Check if the next bucket is a valid location for this entry
                let hash1 = self.hash1(&entry.key, arr_guard.len());
                let hash2 = self.hash2(&entry.key, arr_guard.len());
                
                if path[i + 1] != hash1 && path[i + 1] != hash2 {
                    return false;
                }
            } else {
                // Current bucket is empty, nothing to shift
                return false;
            }
            
            // Move the entry from current bucket to next bucket
            arr_guard[path[i + 1]] = arr_guard[path[i]].take();
        }
        if arr_guard[path[0]].is_some() {
            return false;
        }
        arr_guard[path[0]] = Some(new_entry.clone());
        true
    }

    fn get_vec(&self) -> Vec<KeyVal<K, V>> {
        let arr_guard = self.arr.lock().unwrap();
        
        arr_guard
            .iter()
            .filter_map(|elem| elem.clone())
            .collect()
    }

    fn get_capacity(&self) -> usize {
        self.arr.lock().unwrap().len()
    }
}

impl<K, V> CuckooHashTable<K, V>
where
    K: Hash + Eq + Clone + Debug,
    V: Clone + Debug,
{
    pub fn new() -> Self {
        Self::with_capacity(INIT_SIZE)
    }

    fn with_capacity(capacity: usize) -> Self {
        Self {
            table: RwLock::new(InnerTable {
                arr: Mutex::new((0..capacity).map(|_| None).collect()),
            }),
        }
    }

    pub fn insert(&self, key: K, value: V) {
        let keyval = KeyVal { key, value };
        loop {
            let table = self.table.read().unwrap();
            // Try to insert the entry into one of the 2 possible buckets
            if table.try_direct_insert(&keyval) {
                return;
            }
            // If they're taken, try to shift entries around to make room for it
            if let Some(path) = table.find_insert_path(&keyval.key) {
                if table.try_shift_entries(&path, &keyval) {
                    return;
                }
            // If that's not possible, grow the table, and switch hash
            } else {
                let cap = table.get_capacity();
                drop(table);
                self.resize(cap);
            }
        }
    }

    pub fn get_capacity(&self) -> usize {
        let table = self.table.read().unwrap();
        table.get_capacity()
    }

    fn resize(&self, prev_cap: usize) {
        let mut table = self.table.write().unwrap();
        if prev_cap == table.get_capacity() {
            let elems = table.get_vec();
            let new_table = Self::with_capacity(table.arr.lock().unwrap().len() * 2);
            for entry in elems {
                new_table.insert(entry.key, entry.value);
            }
            *table = new_table.table.into_inner().unwrap();
        }
    }

    pub fn lookup(&self, key: &K) -> Option<V> {
        let table = self.table.read().unwrap();

        let arr_guard = table.arr.lock().unwrap();
        let index1 = table.hash1(key, arr_guard.len());
        let index2 = table.hash2(key, arr_guard.len());

        let bucket1 = &arr_guard[index1];
        if let Some(entry1) = bucket1 {
            if entry1.key == *key {
                return Some(entry1.value.clone());
            }
        }

        let bucket2 = &arr_guard[index2];
        if let Some(entry2) = bucket2 {
            if entry2.key == *key {
                return Some(entry2.value.clone());
            }
        }

        None
    }

    pub fn remove(&self, key: &K) -> Option<V> {
        let table = self.table.read().unwrap();
        let mut arr_guard = table.arr.lock().unwrap();

        let index1 = table.hash1(key, arr_guard.len());
        let index2 = table.hash2(key, arr_guard.len());
        
        if let Some(entry) = &arr_guard[index1] {
            if entry.key == *key {
                // Found in the first bucket, remove it
                let taken = arr_guard[index1].take().unwrap();
                return Some(taken.value);
            }
        }
        
        if index1 != index2 {
            if let Some(entry) = &arr_guard[index2] {
                if entry.key == *key {
                    // Found in the second bucket, remove it
                    let taken = arr_guard[index2].take().unwrap();
                    return Some(taken.value);
                }
            }
        }
        
        None
    }

    pub fn get_vec(&self) -> Vec<KeyVal<K, V>> {
        let table = self.table.read().unwrap();
        table.get_vec()
    }
}

impl<K, V> Default for CuckooHashTable<K, V>
where
    K: Hash + Eq + Clone + Debug,
    V: Clone + Debug,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> Debug for CuckooHashTable<K, V>
where
    K: Hash + Eq + Clone + Debug,
    V: Clone + Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "[")?;

        for elem in self.get_vec().iter() {
            writeln!(f, "({:?}, {:?}),", elem.key, elem.value)?;
        }
        write!(f, "]")
    }
}
