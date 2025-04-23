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

const INIT_SIZE: usize = 400_000;
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

    /// Returns the locked buckets at `idx_a` and `idx_b`.
    /// If `idx_a == idx_b`, returns a single locked bucket.
    /// Does the locking in ascending order to avoid deadlock.
    fn lock_buckets(&self, idx_a: usize, idx_b: usize) -> Vec<MutexGuard<Option<KeyVal<K, V>>>> {
        if idx_a == idx_b {
            vec![self.arr[idx_a].lock().unwrap()]
        } else if idx_a <= idx_b {
            let bucket_a = self.arr[idx_a].lock().unwrap();
            let bucket_b = self.arr[idx_b].lock().unwrap();
            vec![bucket_a, bucket_b]
        } else {
            let bucket_b = self.arr[idx_b].lock().unwrap();
            let bucket_a = self.arr[idx_a].lock().unwrap();
            vec![bucket_a, bucket_b]
        }
    }

    fn try_direct_insert(&self, new_entry: &KeyVal<K, V>) -> bool {
        // The 2 buckets into which this new entry could go.
        let mut locked_buckets =
            self.lock_buckets(self.hash1(&new_entry.key), self.hash2(&new_entry.key));

        for bucket in &mut locked_buckets {
            if let Some(entry) = bucket.as_mut() {
                if entry.key == new_entry.key {
                    *entry = new_entry.clone();
                    return true;
                }
            }
        }

        // Otherwise, if there's an unused bucket,
        // place our entry there

        for bucket in &mut locked_buckets {
            if bucket.is_none() {
                **bucket = Some(new_entry.clone());
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

            path.push(next_idx);
        }
        None
    }

    fn try_shift_entries(&self, path: &[usize], new_entry: &KeyVal<K, V>) -> bool {
        // Shift other entries to free up the first bucket in path
        for i in (0..path.len() - 1).rev() {
            let mut locked_buckets = self.lock_buckets(path[i], path[i + 1]);

            // We can't shift into an occupied bucket
            if locked_buckets[1].is_some() {
                return false;
            }

            // We can't shift into the wrong bucket
            if !self.is_valid_index(&locked_buckets[0], path[i + 1]) {
                return false;
            }

            *locked_buckets[1] = locked_buckets[0].take();
        }
        let mut first_ele = self.arr[path[0]].lock().unwrap();
        if first_ele.is_some() {
            return false;
        }
        *first_ele = Some(new_entry.clone());
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
                table.try_shift_entries(&path, &keyval);
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

        let locked_buckets = table.lock_buckets(table.hash1(key), table.hash2(key));

        for bucket in locked_buckets {
            if let Some(entry) = bucket.as_ref() {
                if entry.key == *key {
                    return Some(entry.value.clone());
                }
            }
        }

        None
    }

    pub fn remove(&self, key: &K) -> Option<V> {
        let table = self.table.read().unwrap();

        let locked_buckets = table.lock_buckets(table.hash1(key), table.hash2(key));

        for mut bucket in locked_buckets {
            let taken = bucket.take_if(|kv| kv.key == *key);
            if let Some(taken) = taken {
                return Some(taken.value);
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
