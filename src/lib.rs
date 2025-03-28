#![warn(clippy::all)]
use std::fmt::Debug;
use std::hash::{Hash, Hasher};

#[cfg(loom)]
use loom::sync::{Mutex, MutexGuard, RwLock};

#[cfg(not(loom))]
use std::sync::{Mutex, MutexGuard, RwLock};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyVal<K, V> {
    pub key: K,
    pub value: V,
}

const INIT_SIZE: usize = 13;
const MAX_RELOCS: usize = 8;

pub struct CuckooHashTable<K, V> {
    table: RwLock<InnerTable<K, V>>,
}

struct InnerTable<K, V> {
    arr: Vec<Mutex<Option<KeyVal<K, V>>>>,
}

impl<K, V> InnerTable<K, V>
where
    K: Hash + Eq + Clone + Debug,
    V: Clone + Debug,
{
    fn hash1(&self, key: &K) -> usize {
        let mut hasher = xxhash_rust::xxh3::Xxh3::with_seed(self.arr.len() as u64);
        key.hash(&mut hasher);
        (hasher.finish() % self.arr.len() as u64) as usize
    }

    fn hash2(&self, key: &K) -> usize {
        let mut hasher = xxhash_rust::xxh3::Xxh3::with_seed(self.arr.len() as u64 + 1);
        key.hash(&mut hasher);
        (hasher.finish() % self.arr.len() as u64) as usize
    }

    fn try_direct_insert(&self, new_entry: &KeyVal<K, V>) -> bool {
        // The 2 buckets into which this new entry could go.
        let mut bucket1 = self.arr[self.hash1(&new_entry.key)].lock().unwrap();

        let mut bucket2 = if self.hash1(&new_entry.key) != self.hash2(&new_entry.key) {
            Some(self.arr[self.hash2(&new_entry.key)].lock().unwrap())
        } else {
            None
        };

        // If this key is already in the table,
        // replace it
        if let Some(entry1) = bucket1.as_ref() {
            if entry1.key == new_entry.key {
                *bucket1 = Some(new_entry.clone());
                return true;
            }
        }
        if let Some(ref mut bucket2) = bucket2 {
            if let Some(entry2) = bucket2.as_ref() {
                if entry2.key == new_entry.key {
                    **bucket2 = Some(new_entry.clone());
                    return true;
                }
            }
        }

        // Otherwise, if there's an unused bucket,
        // place our entry there
        if bucket1.is_none() {
            *bucket1 = Some(new_entry.clone());
            return true;
        }
        if let Some(ref mut bucket2) = bucket2 {
            if bucket2.is_none() {
                **bucket2 = Some(new_entry.clone());
                return true;
            }
        }

        false
    }

    fn find_insert_path(&self, key: &K) -> Option<Vec<usize>> {
        if let Some(path) = self.find_path_that_clears_index(self.hash1(key)) {
            return Some(path);
        }
        if let Some(path) = self.find_path_that_clears_index(self.hash2(key)) {
            return Some(path);
        }
        None
    }

    fn find_path_that_clears_index(&self, index: usize) -> Option<Vec<usize>> {
        let mut path = vec![index];

        for _ in 0..MAX_RELOCS {
            let prev_idx = *path.last().unwrap();
            let bucket = self.arr[prev_idx].lock().unwrap();

            let Some(entry) = bucket.as_ref() else {
                // This bucket is empty, so our shift-chain ends!
                return Some(path);
            };

            // Get the alternate bucket of this entry.
            let next_idx = if self.hash1(&entry.key) != prev_idx {
                self.hash1(&entry.key)
            } else {
                self.hash2(&entry.key)
            };

            if next_idx == index {
                return None;
            }

            path.push(next_idx);
        }
        None
    }

    fn try_shift_entries(&self, path: &[usize]) -> bool {
        // Shift other entries to free up the first bucket in path
        for i in (0..path.len() - 1).rev() {
            let mut current_bucket = self.arr[path[i]].lock().unwrap();
            let mut next_bucket = self.arr[path[i + 1]].lock().unwrap();

            // We can't shift into an occupied bucket
            if next_bucket.is_some() {
                return false;
            }

            // We can't shift into the wrong bucket
            if !self.is_valid_index(&current_bucket, path[i + 1]) {
                return false;
            }

            *next_bucket = current_bucket.take();
        }

        true
    }

    fn is_valid_index(&self, entry: &Option<KeyVal<K, V>>, index: usize) -> bool {
        if let Some(entry) = entry.as_ref() {
            let i1 = self.hash1(&entry.key);
            let i2 = self.hash2(&entry.key);
            index == i1 || index == i2
        } else {
            true
        }
    }

    fn get_vec(&self) -> Vec<KeyVal<K, V>> {
        let locked_elems: Vec<MutexGuard<'_, Option<KeyVal<K, V>>>> =
            self.arr.iter().map(|elem| elem.lock().unwrap()).collect();
        locked_elems
            .iter()
            .filter_map(|elem| (*elem).clone())
            .collect()
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
                arr: (0..capacity).map(|_| Mutex::new(None)).collect(),
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
                table.try_shift_entries(&path);
            // If that's not possible, grow the table, and switch hash
            } else {
                drop(table);
                self.resize();
            }
        }
    }

    fn resize(&self) {
        let mut table = self.table.write().unwrap();
        let elems = table.get_vec();
        let new_table = Self::with_capacity(table.arr.len() * 2);

        for entry in elems {
            new_table.insert(entry.key, entry.value);
        }

        *table = new_table.table.into_inner().unwrap();
    }

    pub fn lookup(&self, key: &K) -> Option<V> {
        let table = self.table.read().unwrap();
        let index1 = table.hash1(key);
        let index2 = table.hash2(key);

        let bucket1 = table.arr[index1].lock().unwrap();
        if let Some(entry1) = bucket1.as_ref() {
            if entry1.key == *key {
                return Some(entry1.value.clone());
            }
        }
        drop(bucket1);

        let bucket2 = table.arr[index2].lock().unwrap();
        if let Some(entry2) = bucket2.as_ref() {
            if entry2.key == *key {}
        }

        None
    }

    pub fn remove(&self, key: &K) -> Option<V> {
        let table = self.table.read().unwrap();
        let index1 = table.hash1(key);
        let index2 = table.hash2(key);

        let mut entry1 = table.arr[index1].lock().unwrap();
        let taken = entry1.take_if(|kv| kv.key == *key);
        if let Some(taken) = taken {
            return Some(taken.value);
        }
        drop(entry1);

        let mut entry2 = table.arr[index2].lock().unwrap();
        let taken = entry2.take_if(|kv| kv.key == *key);
        if let Some(taken) = taken {
            return Some(taken.value);
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
