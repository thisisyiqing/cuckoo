use vstd::atomic_ghost::*;
use vstd::cell::*;
use vstd::modes::*;
use vstd::prelude::*;
use vstd::{pervasive::*, *};
use std::sync::Arc;
use vstd::thread::*;
#[allow(unused_imports)]
use builtin::*;
#[allow(unused_imports)]
use builtin_macros::*;

use rapidhash::rapidhash;
use bincode;
use serde::Serialize;
use gxhash::gxhash64;

verus! {

use state_machines_macros::tokenized_state_machine;

tokenized_state_machine!{CuckooHashTable<T> {
    fields {
        #[sharding(constant)]
        pub backing_cells: Seq<CellId>,

        #[sharding(storage_map)]
        pub storage: Map<nat, cell::PointsTo<T>>,

        #[sharding(variable)]
        pub checked_out_bitmap: nat,

        #[sharding(variable)]
        pub inserted_bitmap: nat,
    }

    pub open spec fn len(&self) -> nat {
        self.backing_cells.len()
    }

    pub open spec fn is_checked_out(&self, i: nat) -> bool {
        0 <= i && i < self.len() &&
        self.checked_out_bitmap >> i == 1
    }

    pub open spec fn is_inserted(&self, i: nat) -> bool {
        0 <= i && i < self.len() &&
        self.inserted_bitmap >> i == 1
    }

    pub open spec fn valid_storage_at_idx(&self, i: nat) -> bool {
        if self.is_checked_out(i) {
            !self.storage.dom().contains(i)
        } else {
            self.storage.dom().contains(i)
            && self.storage.index(i)@.pcell === self.backing_cells.index(i as int)
            && if self.is_inserted(i) {
                self.storage.index(i)@.value.is_Some()
            } else {
                self.storage.index(i)@.value.is_None()
            }
        }
    }

    #[invariant]
    pub fn valid_storage_all(&self) -> bool {
        forall|i: nat| 0 <= i && i < self.len() ==>
            self.valid_storage_at_idx(i)
    }

    init!{
        initialize(backing_cells: Seq<CellId>, storage: Map<nat, cell::PointsTo<T>>) {
            require(
                (forall|i: nat| 0 <= i && i < backing_cells.len() ==>
                    #[trigger] storage.dom().contains(i)
                    && storage.index(i)@.pcell === backing_cells.index(i as int)
                    && storage.index(i)@.value.is_None())
            );
            require(backing_cells.len() > 0);

            init backing_cells = backing_cells;
            init storage = storage;
            init checked_out_bitmap = 0;
            init inserted_bitmap = 0;
        }
    }

    transition!{
        check_out_perm(i: nat) {
            assert(0 <= i && i < pre.backing_cells.len());
            let checked_out_bitmap = pre.checked_out_bitmap;

            withdraw storage -= [i => let perm] by {
                assert(pre.valid_storage_at_idx(i));
            };
            update checked_out_bitmap = (checked_out_bitmap |= 1 << i);

            assert(
                perm@.pcell === pre.backing_cells.index(i)
            ) by {
                assert(pre.valid_storage_at_idx(i));
            };
        }
    }

    transition!{
        return_perm(i: nat, perm: cell::PointsTo<T>) {
            assert(0 <= i && i < pre.backing_cells.len());
            require(is_checked_out(i));
            require(perm@.pcell === pre.backing_cells.index(i));
            
            let checked_out_bitmap = pre.checked_out_bitmap;

            deposit storage += [i => perm] by { assert(pre.valid_storage_at_idx(i)); };

            update checked_out_bitmap = (checked_out_bitmap &= ~(1 << i));
        }
    }

    // transition!{
    //     insert_to_idx(i: nat) {
    //         assert(0 <= i && i < pre.len());
    //         self.inserted_bitmap |= 1 << i;
    //     }
    // }

    // transition!{
    //     remove_from_idx(i: nat) {
    //         assert(0 <= i && i < pre.len());
    //         self.inserted_bitmap &= ~(1 << i);
    //     }
    // }

}}

struct_with_invariants!{
    struct Lock<T> {
        pub atomic: AtomicBool<_, Option<cell::PointsTo<T>>, _>,
        pub cell: PCell<T>,
    }

    spec fn wf(self) -> bool {
        invariant on atomic with (cell)
            is (v: bool, g: Option<cell::PointsTo<T>>)
        {
            match g {
                None => v == true,
                Some(points_to) => points_to.id() == cell.id() && points_to.is_init() && v == false,
            }
        }
    }
}

// impl<T> Lock<T> {
//     fn new(t: T) -> (lock: Self)
//         ensures lock.wf()
//     {
//         let (cell, Tracked(cell_perm)) = PCell::new(t);
//         let atomic = AtomicBool::new(Ghost(cell), false, Tracked(Some(cell_perm)));
//         Lock { atomic, cell }
//     }

//     fn new(cell: PCell<T>, cell_perm: Tracked<cell::PointsTo<T>>) -> (lock: Self)
//         ensures lock.wf()
//     {
//         let atomic = AtomicBool::new(Ghost(cell), false, Tracked(Some(cell_perm)));
//         Lock { atomic, cell }
//     }

//     fn acquire(&self) -> (points_to: Tracked<cell::PointsTo<T>>)
//         requires self.wf(),
//         ensures points_to@.id() == self.cell.id(), points_to@.is_init()
//     {
//         loop
//             invariant self.wf(),
//         {
//             let tracked mut points_to_opt = None;
//             let res = atomic_with_ghost!(&self.atomic => compare_exchange(false, true);
//                 ghost g => {
//                     tracked_swap(&mut points_to_opt, &mut g);
//                 }
//             );
//             if res.is_ok() {
//                 return Tracked(points_to_opt.tracked_unwrap());
//             }
//         }
//     }

//     fn release(&self, Tracked(points_to): Tracked<cell::PointsTo<T>>)
//         requires self.wf(), points_to.id() == self.cell.id(), points_to.is_init()
//     {
//         atomic_with_ghost!(&self.atomic => store(false);
//             ghost g => {
//                 g = Some(points_to);
//             }
//         );
//     }
// }

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

struct_with_invariants!{
    struct HashTable<K, V> {
        buffer: Vec<PCell<KeyVal<K, V>>>,
        checked_out_bitmap_atomic: AtomicU64<_, CuckooHashTable::checked_out_bitmap<KeyVal<K, V>>, _>,
        inserted_bitmap_atomic: AtomicU64<_, CuckooHashTable::inserted_bitmap<KeyVal<K, V>>, _>,

        instance: Tracked<CuckooHashTable::Instance<KeyVal<K, V>>>,
    }

    pub closed spec fn wf(&self) -> bool {
        predicate {
            &&& self.instance@.backing_cells().len() == self.buffer@.len()
            &&& forall|i: int| 0 <= i && i < self.buffer@.len() as int ==>
                self.instance@.backing_cells().index(i) ===
                    self.buffer@.index(i).id()
        }

        invariant on checked_out_bitmap_atomic with (instance) is (v: u64, g: CuckooHashTable::checked_out_bitmap_atomic<KeyVal<K, V>>) {
            &&& g@.instance === instance@
            &&& g@.value == v as int
        }

        invariant on inserted_bitmap_atomic with (instance) is (v: u64, g: CuckooHashTable::inserted_bitmap_atomic<KeyVal<K, V>>) {
            &&& g@.instance === instance@
            &&& g@.value == v as int
        }
    }
}

// pub fn new_ht<K, V>(len: usize) -> Arc<HashTable<K, V>> {
//     let mut backing_cells_vec = Vec::<Lock<KeyVal<K, V>>>::new();

//     let tracked mut perms = Map::<nat, cell::PointsTo<KeyVal<K, V>>>::tracked_empty();
//     while backing_cells_vec.len() < len
//         invariant
//             forall|j: nat|
//                 #![trigger( perms.dom().contains(j) )]
//                 #![trigger( backing_cells_vec@.index(j as int) )]
//                 #![trigger( perms.index(j) )]
//                 0 <= j && j < backing_cells_vec.len() as int ==> perms.dom().contains(j)
//                     && backing_cells_vec@.index(j as int).cell.id() === perms.index(j)@.pcell
//                     && perms.index(j)@.value.is_None(),
//     {
//         let ghost i = backing_cells_vec.len();
//         let (cell, cell_perm) = PCell::empty();
//         let lock = Lock<KeyVal<K, V>>::new(cell, cell_perm);
//         backing_cells_vec.push(lock);
//         proof {
//             perms.tracked_insert(i as nat, cell_perm.get());
//         }
//         assert(perms.dom().contains(i as nat));
//         assert(backing_cells_vec@.index(i as int).cell.id() === perms.index(i as nat)@.pcell);
//         assert(perms.index(i as nat)@.value.is_None());
//     }

//     let ghost mut backing_cells_ids = Seq::<CellId>::new(
//         backing_cells_vec@.len(),
//         |i: int| backing_cells_vec@.index(i).cell.id(),
//     );

//     let tracked (
//         Tracked(instance),
//         Tracked(checked_out_bitmap_token),
//         Tracked(inserted_bitmap_token),
//     ) = CuckooHashTable::Instance::initialize(backing_cells_ids, perms);
    
//     let tracked_inst: Tracked<CuckooHashTable::Instance<KeyVal<K, V>>> = Tracked(instance.clone());
//     let checked_out_bitmap_atomic = AtomicU64::new(Ghost(tracked_inst), 0, Tracked(checked_out_bitmap_token));
//     let inserted_bitmap_atomic = AtomicU64::new(Ghost(tracked_inst), 0, Tracked(inserted_bitmap_token));

//     let ht = HashTable::<K, V> {
//         instance: Tracked(instance),
//         buffer: backing_cells_vec,
//         checked_out_bitmap_atomic: checked_out_bitmap_atomic,
//         inserted_bitmap_atomic: inserted_bitmap_atomic,
//     }

//     Arc::new(ht)
// }

pub fn new_ht<K, V>(len: usize) -> Arc<HashTable<K, V>> {
    let mut backing_cells_vec = Vec::<PCell<KeyVal<K, V>>>::new();

    let tracked mut perms = Map::<nat, cell::PointsTo<KeyVal<K, V>>>::tracked_empty();
    while backing_cells_vec.len() < len
        invariant
            forall|j: nat|
                #![trigger( perms.dom().contains(j) )]
                #![trigger( backing_cells_vec@.index(j as int) )]
                #![trigger( perms.index(j) )]
                0 <= j && j < backing_cells_vec.len() as int ==> perms.dom().contains(j)
                    && backing_cells_vec@.index(j as int).id() === perms.index(j)@.pcell
                    && perms.index(j)@.value.is_None(),
    {
        let ghost i = backing_cells_vec.len();
        let (cell, cell_perm) = PCell::empty();
        backing_cells_vec.push(cell);
        proof {
            perms.tracked_insert(i as nat, cell_perm.get());
        }
        assert(perms.dom().contains(i as nat));
        assert(backing_cells_vec@.index(i as int).id() === perms.index(i as nat)@.pcell);
        assert(perms.index(i as nat)@.value.is_None());
    }

    let ghost mut backing_cells_ids = Seq::<CellId>::new(
        backing_cells_vec@.len(),
        |i: int| backing_cells_vec@.index(i).id(),
    );

    let tracked (
        Tracked(instance),
        Tracked(checked_out_bitmap_token),
        Tracked(inserted_bitmap_token),
    ) = CuckooHashTable::Instance::initialize(backing_cells_ids, perms);
    
    let tracked_inst: Tracked<CuckooHashTable::Instance<KeyVal<K, V>>> = Tracked(instance.clone());
    let checked_out_bitmap_atomic = AtomicU64::new(Ghost(tracked_inst), 0, Tracked(checked_out_bitmap_token));
    let inserted_bitmap_atomic = AtomicU64::new(Ghost(tracked_inst), 0, Tracked(inserted_bitmap_token));

    let ht = HashTable::<K, V> {
        instance: Tracked(instance),
        buffer: backing_cells_vec,
        checked_out_bitmap_atomic: checked_out_bitmap_atomic,
        inserted_bitmap_atomic: inserted_bitmap_atomic,
    }

    Arc::new(ht)
}

impl<K, V> HashTable<K, V> {
    fn hash1(&self, key: &K) -> usize {
        let key_bytes = bincode::serialize(key).unwrap();
        rapidhash(key_bytes.as_slice()) as usize % self.buffer.len()
    }

    fn hash2(&self, key: &K) -> usize {
        let key_bytes = bincode::serialize(key).unwrap();
        gxhash64(key_bytes.as_slice(), 14893) as usize % self.buffer.len()
    }

    pub fn insert_find_path(&self, key: &K) -> Option<Vec<usize>>
        requires
            old(self).wf(),
        ensures
            self.wf(),
    {
        let mut queue = vec![vec![self.hash1(key)], vec![self.hash2(key)]];

        while let Some(path) = queue.pop()
            invariant
                self.wf(),
                queue.len() >= 0,
            ensures
                self.wf(),
                queue.len() >= 0,
        {
            if path.len() > MAX_RELOCS {
                break;
            }

            let last_idx = *path.last().unwrap();
            let tracked cell_perm = atomic_with_ghost!(&self.checked_out_bitmap_atomic => load(); ghost checked_out_bitmap_token => {
                self.instance.borrow().check_out_perm(&mut checked_out_bitmap_token);
            });

            // let mut cell_perm = self.buffer[last_idx].acquire();
            let entry = *self.buffer[last_idx].borrow(Tracked(cell_perm.borrow()));
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
}


fn main() {
    // let lock = Lock::new(10 as u64);
    // let mut cell_perm = lock.acquire();
    // print_u64(lock.cell.take(Tracked(cell_perm.borrow_mut())));
    // lock.cell.put(Tracked(cell_perm.borrow_mut()), 20);
    // print_u64(*lock.cell.borrow(Tracked(cell_perm.borrow())));
    // lock.release(cell_perm);

    // let lock = Arc::new(Lock::new(0 as u64));
    // let mut handles = Vec::new();
    // let mut i = 0;
    // while i < 13
    //     invariant
    //         (*lock).wf(),
    // {
    //     let l = Arc::clone(&lock);

    //     let handle = spawn(move ||
    //     {
    //         let mut cell_perm = (*l).acquire();
    //         let mut prev = (*l).cell.take(Tracked(cell_perm.borrow_mut()));
    //         if prev == u64::MAX {
    //             prev = u64::MAX - 1
    //         }
    //         (*l).cell.put(Tracked(cell_perm.borrow_mut()), prev + 1);
    //         (*l).release(cell_perm);
    //     });

    //     i = i + 1;
    //     handles.push(handle);
    // }

    // i = 0;
    // while i < 13 {
    //     match handles.pop() {
    //         Some(handle) => {
    //             match handle.join() {
    //                 Result::Ok(prev) => {
    //                 },
    //                 _ => {
    //                     return;
    //                 },
    //             };
    //         },
    //         None => {
    //             return;
    //         },
    //     }
    //     i = i + 1;
    // }

    // let mut cell_perm = (*lock).acquire();
    // print_u64(*lock.cell.borrow(Tracked(cell_perm.borrow())));
    // (*lock).release(cell_perm);

    let mut ht = new_ht<&str, &str>(32);
    let path = ht.insert_find_path("key1");
    println!("Path is {}", path);
}
}