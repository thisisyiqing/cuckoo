use rapidhash::rapidhash;
use bincode;
use serde::Serialize;
use gxhash::gxhash64;

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
    size: usize,
    arr: Vec<KeyVal<K, V>>,
}

impl<K: Serialize + Eq + Clone + std::fmt::Debug, V: Clone + std::fmt::Debug> CuckooHashTable<K, V> {
    pub fn new() -> Self {
        CuckooHashTable {
            size: INIT_SIZE,
            arr: vec![KeyVal::new(); INIT_SIZE],
        }
    }

    fn hash1(&mut self, key: &K) -> usize {
        let key_bytes = bincode::serialize(key).unwrap();
        rapidhash(&key_bytes.as_slice()) as usize % self.size
    }

    fn hash2(&mut self, key: &K) -> usize {
        let key_bytes = bincode::serialize(key).unwrap();
        gxhash64(&key_bytes.as_slice(), 14893) as usize % self.size
    }

    fn insert_find_path(&mut self, key: K) -> Option<Vec<usize>> {
        // need to make sure key is not already in the hashtable

        let mut queue: Vec<Vec<usize>> = Vec::new();
        queue.push(vec![self.hash1(&key)]);
        queue.push(vec![self.hash2(&key)]);

        while queue.first().unwrap().len() <= MAX_RELOCS {
            let mut ele = queue.remove(0);
            let last_idx = ele.last().unwrap();
            let last_idx_key = self.arr[*last_idx].key.clone();
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

    pub fn insert(&mut self, key: K, value: V) -> Result<(), String> {
        loop {
            let path = self.insert_find_path(key.clone());
            if path.is_none() {
                println!("Resizing required");
                self.resize();
            } else {
                let q: Vec<usize> = path.unwrap();
                println!("{:?}", q);
                for i in (0..q.len() - 1).rev() {
                    self.arr[q[i + 1]] = self.arr[q[i]].clone();
                }
                self.arr[q[0]].key = Some(key);
                self.arr[q[0]].value = Some(value);
                return Ok(());
            }
        }
    }

    fn resize(&mut self) {
        let old_table = self.arr.clone();
        self.size *= 2;
        self.arr = vec![KeyVal::new(); self.size];
    
        for entry in old_table.into_iter() {
            if entry.key.is_some() {
                self.insert(entry.key.unwrap(), entry.value.unwrap()).unwrap();
            }
        }
    }

    pub fn lookup(&mut self, key: &K) -> Option<V> {
        let index1 = self.hash1(key);
        if let Some(ref k) = self.arr[index1].key {
            if k == key {
                return self.arr[index1].value.clone();
            }
        }

        let index2 = self.hash2(key);
        if let Some(ref k) = self.arr[index2].key {
            if k == key {
                return self.arr[index2].value.clone();
            }
        }

        None
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let index1 = self.hash1(key);
        if let Some(ref k) = self.arr[index1].key {
            if k == key {
                let value = self.arr[index1].value.take();
                self.arr[index1].key = None;
                return value;
            }
        }

        let index2 = self.hash2(key);
        if let Some(ref k) = self.arr[index2].key {
            if k == key {
                let value = self.arr[index2].value.take();
                self.arr[index2].key = None;
                return value;
            }
        }

        None
    }

    pub fn print_all(&mut self) {
        for i in 0..self.size {
            if let Some(ref k) = self.arr[i].key {
                println!("({:?}, {:?})", k, self.arr[i].value)
            }
        }
    }
}

