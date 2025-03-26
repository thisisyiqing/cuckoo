#![warn(clippy::all)]
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct KeyVal<K, V> {
    pub key: K,
    pub value: V,
}

const INIT_SIZE: usize = 13;
const MAX_RELOCS: usize = 8;

pub struct CuckooHashTable<K, V> {
    size: Arc<Mutex<usize>>,
    arr: Arc<Vec<Mutex<Option<KeyVal<K, V>>>>>,
}

impl<K, V> CuckooHashTable<K, V>
where
    K: Hash + Eq + Clone + Debug,
    V: Clone + Debug,
{
    pub fn new() -> Self {
        Self {
            size: Arc::new(Mutex::new(INIT_SIZE)),
            arr: Arc::new((0..INIT_SIZE).map(|_| Mutex::new(None)).collect()),
        }
    }

    fn hash1(&self, key: &K) -> usize {
        let mut hasher = rapidhash::RapidHasher::default_const();
        key.hash(&mut hasher);
        (hasher.finish() % *self.size.lock().unwrap() as u64) as usize
    }

    fn hash2(&self, key: &K) -> usize {
        let mut hasher = gxhash::GxHasher::with_seed(14893);
        key.hash(&mut hasher);
        (hasher.finish() % *self.size.lock().unwrap() as u64) as usize
    }

    fn insert_find_path(&self, key: &K) -> Option<Vec<usize>> {
        let mut queue = vec![vec![self.hash1(key)], vec![self.hash2(key)]];

        while let Some(path) = queue.pop() {
            if path.len() > MAX_RELOCS {
                break;
            }

            let last_idx = *path.last().unwrap();
            let entry = self.arr[last_idx].lock().unwrap();

            let Some(entry) = entry.as_ref() else {
                return Some(path);
            };

            let next_idx = if self.hash1(&entry.key) != last_idx {
                self.hash1(&entry.key)
            } else {
                self.hash2(&entry.key)
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

            if next_entry.is_some() {
                println!("Path is invalid, retrying...");
                return false;
            }

            if let Some(current_entry) = current_entry.as_ref() {
                if !self.is_valid_move(current_entry, path[i + 1]) {
                    println!("Path is invalid, retrying...");
                    return false;
                }
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

    fn is_valid_move(&self, entry: &KeyVal<K, V>, next_idx: usize) -> bool {
        let i1 = self.hash1(&entry.key);
        let i2 = self.hash2(&entry.key);
        next_idx == i1 || next_idx == i2
    }

    fn resize(&self) -> Result<(), String> {
        let mut size = self
            .size
            .try_lock()
            .map_err(|_| "Another resize in progress, aborting...".to_string())?;
        let old_size = *size;
        let new_size = old_size * 2;

        let old_table: Vec<Option<KeyVal<K, V>>> = (0..old_size)
            .map(|i| self.arr[i].lock().unwrap().clone())
            .collect();

        let new_arr: Vec<Mutex<Option<KeyVal<K, V>>>> =
            (0..new_size).map(|_| Mutex::new(None)).collect();

        unsafe {
            let cloned = &mut Arc::clone(&self.arr);
            let arr = Arc::get_mut_unchecked(cloned);
            *arr = new_arr;
        }

        *size = new_size;
        drop(size);

        for entry in old_table.into_iter().flatten() {
            self.insert(entry.key, entry.value)?;
        }

        Ok(())
    }

    pub fn lookup(&self, key: &K) -> Option<V> {
        let index1 = self.hash1(key);
        let index2 = self.hash2(key);

        let entry1 = self.arr[index1].lock().unwrap();
        if let Some(entry1) = entry1.as_ref() {
            if entry1.key == *key {
                return Some(entry1.value.clone());
            }
        }
        drop(entry1);

        let entry2 = self.arr[index2].lock().unwrap();
        if let Some(entry2) = entry2.as_ref() {
            if entry2.key == *key {
                return Some(entry2.value.clone());
            }
        }

        None
    }

    pub fn remove(&self, key: &K) -> Option<V> {
        let index1 = self.hash1(key);
        let index2 = self.hash2(key);

        let mut entry1 = self.arr[index1].lock().unwrap();
        let taken = entry1.take_if(|kv| kv.key == *key);
        if let Some(taken) = taken {
            return Some(taken.value);
        }
        drop(entry1);

        let mut entry2 = self.arr[index2].lock().unwrap();
        let taken = entry2.take_if(|kv| kv.key == *key);
        if let Some(taken) = taken {
            return Some(taken.value);
        }

        None
    }

    pub fn print_all(&self) {
        let locked_entries: Vec<_> = self.arr.iter().map(|entry| entry.lock().unwrap()).collect();

        for (i, entry) in locked_entries.iter().enumerate() {
            if let Some(entry) = entry.as_ref() {
                println!("({}, {:?}, {:?})", i, entry.key, entry.value);
            }
        }
    }
}
