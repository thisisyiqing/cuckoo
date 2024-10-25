use rapidhash::rapidhash;
use bincode;
use serde::Serialize;
use gxhash::gxhash64;
use std::sync::{Arc, Mutex};
use std::fmt::Debug;

#[derive(Clone, Debug)]
pub struct KeyVal<K, V> {
    pub key: Option<K>,
    pub value: Option<V>,
}

impl<K, V> KeyVal<K, V> {
    pub fn new() -> Self {
        Self { key: None, value: None }
    }
}

const INIT_SIZE: usize = 13;
const MAX_RELOCS: usize = 8;

pub struct CuckooHashTable<K, V> {
    size: Arc<Mutex<usize>>,
    arr: Arc<Vec<Mutex<KeyVal<K, V>>>>,
}

impl<K, V> CuckooHashTable<K, V>
where
    K: Serialize + Eq + Clone + Debug,
    V: Clone + Debug,
{
    pub fn new() -> Self {
        Self {
            size: Arc::new(Mutex::new(INIT_SIZE)),
            arr: Arc::new((0..INIT_SIZE).map(|_| Mutex::new(KeyVal::new())).collect()),
        }
    }

    fn hash1(&self, key: &K) -> usize {
        let key_bytes = bincode::serialize(key).unwrap();
        rapidhash(key_bytes.as_slice()) as usize % *self.size.lock().unwrap()
    }

    fn hash2(&self, key: &K) -> usize {
        let key_bytes = bincode::serialize(key).unwrap();
        gxhash64(key_bytes.as_slice(), 14893) as usize % *self.size.lock().unwrap()
    }

    fn insert_find_path(&self, key: &K) -> Option<Vec<usize>> {
        let mut queue = vec![vec![self.hash1(key)], vec![self.hash2(key)]];

        while let Some(path) = queue.pop() {
            if path.len() > MAX_RELOCS {
                break;
            }

            let last_idx = *path.last().unwrap();
            let entry = self.arr[last_idx].lock().unwrap();

            if entry.key.is_none() {
                return Some(path);
            }

            let next_idx = if self.hash1(entry.key.as_ref().unwrap()) != last_idx {
                self.hash1(entry.key.as_ref().unwrap())
            } else {
                self.hash2(entry.key.as_ref().unwrap())
            };

            let mut new_path = path;
            new_path.push(next_idx);
            queue.push(new_path);
        }

        None
    }

    pub fn insert(&self, key: K, value: V) -> Result<(), String> {
        loop {
            if let Some(path) = self.insert_find_path(&key) {
                println!("{:?} {:?}", key, path);
                
                if self.try_insert_along_path(&path, key.clone(), value.clone()) {
                    return Ok(());
                }
            } else {
                println!("Begin to resize...");
                self.resize()?;
            }
        }
    }

    fn try_insert_along_path(&self, path: &[usize], key: K, value: V) -> bool {
        for i in (0..path.len() - 1).rev() {
            let mut current_entry = self.arr[path[i]].lock().unwrap();
            let mut next_entry = self.arr[path[i + 1]].lock().unwrap();

            if next_entry.key.is_some() || !self.is_valid_move(&current_entry, path[i + 1]) {
                println!("Path is invalid, retrying...");
                return false;
            }

            next_entry.key = current_entry.key.take();
            next_entry.value = current_entry.value.take();
        }

        let mut first_entry = self.arr[path[0]].lock().unwrap();
        if first_entry.key.is_some() {
            println!("Path is invalid, retrying...");
            return false;
        }

        first_entry.key = Some(key);
        first_entry.value = Some(value);
        true
    }

    fn is_valid_move(&self, entry: &KeyVal<K, V>, next_idx: usize) -> bool {
        if let Some(ref key) = entry.key {
            let i1 = self.hash1(key);
            let i2 = self.hash2(key);
            next_idx == i1 || next_idx == i2
        } else {
            false
        }
    }

    fn resize(&self) -> Result<(), String> {
        let mut size = self.size.try_lock().map_err(|_| "Another resize in progress, aborting...".to_string())?;
        let old_size = *size;
        let new_size = old_size * 2;

        let old_table: Vec<KeyVal<K, V>> = (0..old_size)
            .map(|i| self.arr[i].lock().unwrap().clone())
            .collect();

        let new_arr: Vec<Mutex<KeyVal<K, V>>> = (0..new_size).map(|_| Mutex::new(KeyVal::new())).collect();

        unsafe {
            let cloned = &mut Arc::clone(&self.arr);
            let arr = Arc::get_mut_unchecked(cloned);
            *arr = new_arr;
        }

        *size = new_size;
        drop(size);

        for entry in old_table {
            if let Some(key) = entry.key {
                self.insert(key, entry.value.unwrap())?;
            }
        }

        Ok(())
    }

    pub fn lookup(&self, key: &K) -> Option<V> {
        let index1 = self.hash1(key);
        let index2 = self.hash2(key);

        let entry1 = self.arr[index1].lock().unwrap();
        if entry1.key.as_ref() == Some(key) {
            return entry1.value.clone();
        }
        drop(entry1);

        let entry2 = self.arr[index2].lock().unwrap();
        if entry2.key.as_ref() == Some(key) {
            return entry2.value.clone();
        }

        None
    }

    pub fn remove(&self, key: &K) -> Option<V> {
        let index1 = self.hash1(key);
        let index2 = self.hash2(key);

        let mut entry1 = self.arr[index1].lock().unwrap();
        if entry1.key.as_ref() == Some(key) {
            entry1.key = None;
            return entry1.value.take();
        }
        drop(entry1);

        let mut entry2 = self.arr[index2].lock().unwrap();
        if entry2.key.as_ref() == Some(key) {
            entry2.key = None;
            return entry2.value.take();
        }

        None
    }

    pub fn print_all(&self) {
        let locked_entries: Vec<_> = self.arr.iter()
            .map(|entry| entry.lock().unwrap())
            .collect();

        for (i, entry) in locked_entries.iter().enumerate() {
            if let Some(ref k) = entry.key {
                println!("({}, {:?}, {:?})", i, k, entry.value);
            }
        }
    }
}

