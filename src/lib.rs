#![warn(clippy::all)]
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, MutexGuard, RwLock};

#[derive(Clone, Debug)]
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
        let mut hasher = rapidhash::RapidHasher::new(self.arr.len() as u64);
        key.hash(&mut hasher);
        (hasher.finish() % self.arr.len() as u64) as usize
    }

    fn hash2(&self, key: &K) -> usize {
        let mut hasher = gxhash::GxHasher::with_seed(self.arr.len() as i64);
        key.hash(&mut hasher);
        (hasher.finish() % self.arr.len() as u64) as usize
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
            let entry = self.arr[prev_idx].lock().unwrap();

            let Some(entry) = entry.as_ref() else {
                return Some(path);
            };

            let next_idx = if self.hash1(&entry.key) != prev_idx {
                self.hash1(&entry.key)
            } else {
                self.hash2(&entry.key)
            };

            path.push(next_idx);
        }
        None
    }

    fn try_insert_along_path(&self, path: &[usize], key: K, value: V) -> bool {
        for i in (0..path.len() - 1).rev() {
            let mut current_entry = self.arr[path[i]].lock().unwrap();
            let mut next_entry = self.arr[path[i + 1]].lock().unwrap();

            if next_entry.is_some() {
                println!("Path is invalid, retrying...");
                return false;
            }

            if !self.is_valid_move(&current_entry, path[i + 1]) {
                println!("Path is invalid, retrying...");
                return false;
            }

            *next_entry = current_entry.take();
        }

        let mut first_entry = self.arr[path[0]].lock().unwrap();

        if first_entry.is_some() {
            println!("Path is invalid, retrying...");
            return false;
        }

        *first_entry = Some(KeyVal { key, value });
        true
    }

    fn is_valid_move(&self, entry: &Option<KeyVal<K, V>>, next_idx: usize) -> bool {
        if let Some(entry) = entry.as_ref() {
            let i1 = self.hash1(&entry.key);
            let i2 = self.hash2(&entry.key);
            next_idx == i1 || next_idx == i2
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

    pub fn insert(&self, key: K, value: V) -> Result<(), String> {
        loop {
            let table = self.table.read().unwrap();
            if let Some(path) = table.find_insert_path(&key) {
                println!("{:?} {:?}", key, path);

                if table.try_insert_along_path(&path, key.clone(), value.clone()) {
                    return Ok(());
                }
            } else {
                drop(table);
                println!("Begin to resize...");
                self.resize()?;
            }
        }
    }

    fn resize(&self) -> Result<(), String> {
        let mut table = self.table.write().unwrap();

        let new_table = Self::with_capacity(table.arr.len() * 2);

        let elems = table.get_vec();

        for entry in elems {
            new_table.insert(entry.key, entry.value)?;
        }

        *table = new_table.table.into_inner().unwrap();

        Ok(())
    }

    pub fn lookup(&self, key: &K) -> Option<V> {
        let table = self.table.read().unwrap();
        let index1 = table.hash1(key);
        let index2 = table.hash2(key);

        let entry1 = table.arr[index1].lock().unwrap();
        if let Some(entry1) = entry1.as_ref() {
            if entry1.key == *key {
                return Some(entry1.value.clone());
            }
        }
        drop(entry1);

        let entry2 = table.arr[index2].lock().unwrap();
        if let Some(entry2) = entry2.as_ref() {
            if entry2.key == *key {
                return Some(entry2.value.clone());
            }
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

impl<K, V> Debug for CuckooHashTable<K, V>
where
    K: Hash + Eq + Clone + Debug,
    V: Clone + Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "[")?;

        for elem in self.get_vec().iter().skip(1) {
            writeln!(f, "({:?}, {:?}),", elem.key, elem.value)?;
        }
        write!(f, "]")
    }
}
