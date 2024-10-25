use rapidhash::rapidhash;
use bincode;
use serde::Serialize;
use gxhash::gxhash64;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct KeyVal<K, V> {
    pub key: Option<K>,
    pub value: Option<V>,
}

impl<K, V> KeyVal<K, V> {
    pub fn new() -> Self {
        KeyVal {
            key: None,
            value: None,
        }
    }
}

const INIT_SIZE: usize = 12;
const MAX_RELOCS: usize = 12;

pub struct CuckooHashTable<K, V> {
    size: Arc<Mutex<usize>>,
    arr: Arc<Vec<Mutex<KeyVal<K, V>>>>,
}

impl<K: Serialize + Eq + Clone + std::fmt::Debug, V: Clone + std::fmt::Debug> CuckooHashTable<K, V> {
    pub fn new() -> Self {
        CuckooHashTable {
            size: Arc::new(Mutex::new(INIT_SIZE)),
            arr: Arc::new((0..INIT_SIZE).map(|_| Mutex::new(KeyVal::new())).collect())
        }
    }

    fn hash1(&self, key: &K) -> usize {
        let key_bytes = bincode::serialize(key).unwrap();
        rapidhash(&key_bytes.as_slice()) as usize % *self.size.lock().unwrap()
    }

    fn hash2(&self, key: &K) -> usize {
        let key_bytes = bincode::serialize(key).unwrap();
        gxhash64(&key_bytes.as_slice(), 14893) as usize % *self.size.lock().unwrap()
    }

    fn insert_find_path(&self, key: K) -> Option<Vec<usize>> {
        let mut queue: Vec<Vec<usize>> = Vec::new();
        queue.push(vec![self.hash1(&key)]);
        queue.push(vec![self.hash2(&key)]);

        while queue.first().unwrap().len() <= MAX_RELOCS {
            let mut ele = queue.remove(0);
            let last_idx = ele.last().unwrap();
            let last_idx_key;
            // Lock the mutex for the last index
            {
                let entry = self.arr[*last_idx].lock().unwrap();
                last_idx_key = entry.key.clone();
            }

            if last_idx_key.is_none() {
                return Some(ele);
            }

            let next_idx = if self.hash1(&last_idx_key.as_ref().unwrap()) != *last_idx {
                self.hash1(&last_idx_key.as_ref().unwrap())
            } else {
                self.hash2(&last_idx_key.as_ref().unwrap())
            };

            ele.push(next_idx);
            queue.push(ele);
        }

        None
    }

    pub fn insert(&self, key: K, value: V) -> Result<(), String> {
        loop {
            let path: Option<Vec<usize>> = self.insert_find_path(key.clone());
            if path.is_none() {
                self.resize();
            } else {
                let q: Vec<usize> = path.unwrap();
                println!("{:?} {:?}", key, q);
                let mut invalid = false;
                for i in (0..q.len() - 1).rev() {
                    let mut current_entry = self.arr[q[i]].lock().unwrap();
                    let mut next_entry = self.arr[q[i + 1]].lock().unwrap();
                    if next_entry.key.is_some() {
                        invalid = true;
                        println!("path is invalid so retrying...");
                        break;
                        // return Err("path is invalid".to_string());
                    }
                    let i1 = self.hash1(current_entry.key.as_ref().unwrap());
                    let i2 = self.hash2(current_entry.key.as_ref().unwrap());
                    if !(q[i + 1] == i1 || q[i + 1] == i2) {
                        invalid = true;
                        println!("path is invalid so retrying...");
                        break;
                        // return Err("path is invalid".to_string());
                    }
                    next_entry.key = current_entry.key.clone();
                    next_entry.value = current_entry.value.clone();
                    current_entry.key = None;
                }
                if invalid {
                    continue;
                }

                {
                    let mut first_entry = self.arr[q[0]].lock().unwrap();
                    if first_entry.key.is_some() {
                        println!("path is invalid so retrying...");
                        continue;
                    }
                    first_entry.key = Some(key);
                    first_entry.value = Some(value);
                }

                return Ok(());
            }
        }
    }

    fn resize(&self) -> Result<(), String> {
        match self.size.try_lock() {
            Ok(_) => {}
            Err(_) => {
                return Err("another resize in progress, aborting...".to_string());
            }
        }
        let mut s = self.size.lock().unwrap();
        let old_table: Vec<KeyVal<K, V>>;
        {
            // Collect old entries while holding their locks
            old_table = (0..*s)
                .map(|i| {
                    let entry: std::sync::MutexGuard<'_, KeyVal<K, V>> = self.arr[i].lock().unwrap();
                    entry.clone()
                })
                .collect();
        }

        // Resize the array
        let new_size = *s * 2;
        let new_arr: Vec<Mutex<KeyVal<K, V>>> = (0..new_size).map(|_| Mutex::new(KeyVal::new())).collect();

        // Update self.arr to the new array
        // Lock for exclusive access during resizing
        unsafe {
            let cloned = &mut Arc::clone(&self.arr);
            let arr = Arc::get_mut_unchecked(cloned);
            *arr = new_arr;
        }
        *s = new_size;
        drop(s);
        // Reinsert the old table's entries
        for entry in old_table.into_iter() {
            if entry.key.is_some() {
                self.insert(entry.key.unwrap(), entry.value.unwrap()).unwrap();
            }
        }
        return Ok(());
    }

    pub fn lookup(&self, key: &K) -> Option<V> {
        let index1 = self.hash1(key);
        let index2 = self.hash2(key);

        let entry1 = self.arr[index1].lock().unwrap();
        let entry2 = self.arr[index2].lock().unwrap();
        if let Some(ref k) = entry1.key {
            if k == key {
                return entry1.value.clone();
            }
        }
        drop(entry1);

        if let Some(ref k) = entry2.key {
            if k == key {
                return entry2.value.clone();
            }
        }

        None
    }

    pub fn remove(&self, key: &K) -> Option<V> {
        let index1 = self.hash1(key);
        let index2 = self.hash2(key);

        let mut entry1 = self.arr[index1].lock().unwrap();
        let mut entry2 = self.arr[index2].lock().unwrap();
        if let Some(ref k) = entry1.key {
            if k == key {
                let value = entry1.value.take();
                entry1.key = None;
                return value;
            }
        }
        drop(entry1);
            
        if let Some(ref k) = entry2.key {
            if k == key {
                let value = entry2.value.take();
                entry2.key = None;
                return value;
            }
        }

        None
    }

    pub fn print_all(&self) {
    // Collect all locks first
    let locked_entries: Vec<_> = self.arr.iter()
        .map(|entry| entry.lock().unwrap())
        .collect();

    // Now print the entries
    for (i, entry) in locked_entries.iter().enumerate() {
        if let Some(ref k) = entry.key {
            println!("({}, {:?}, {:?})", i, k, entry.value);
        }
    }
}
}

