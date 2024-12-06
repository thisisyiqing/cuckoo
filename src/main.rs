use vstd::atomic_ghost::*;
use vstd::cell::*;
use vstd::prelude::*;
use vstd::*;
#[allow(unused_imports)]
use builtin::*;
#[allow(unused_imports)]
use builtin_macros::*;
use std::string::String;
use vstd::pervasive::*;

// TODO: verus does not support importing third-party crates?
// use rapidhash::rapidhash;
// use bincode;
// use serde::Serialize;
// use gxhash::gxhash64;

verus! {

use state_machines_macros::tokenized_state_machine;

tokenized_state_machine!{CuckooHashTable<T> {
    fields {
        // Store the unique identifier of a cell at its corresponding index
        #[sharding(constant)]
        pub backing_cells: Seq<CellId>,

        // Store the permission object of a cell, and the key is the index of the cell
        #[sharding(storage_map)]
        pub storage: Map<nat, cell::PointsTo<T>>,
        
        // When a cell's permission is checked out from the storage map above, we flip its bit in the bitmap at the corresponding index.
        #[sharding(variable)]
        pub checked_out_bitmap: nat,

        // When a cell has a non-None key and value, it is considered inserted.
        // When a cell is inserted or removed, we flip its bit in the bitmap at the corresponding index.
        // TODO: not used in the proof so far
        #[sharding(variable)]
        pub inserted_bitmap: nat,
    }

    // Return the length of the hashtable that this state machine is keeping track of
    // TODO: need to be a fixed length so we cannot resize for now
    pub open spec fn len(&self) -> nat {
        self.backing_cells.len()
    }

    pub open spec fn is_checked_out(&self, i: nat) -> bool {
        0 <= i && i < self.len() &&
        self.checked_out_bitmap as i64 >> i == 1
    }

    pub open spec fn is_inserted(&self, i: nat) -> bool {
        0 <= i && i < self.len() &&
        self.inserted_bitmap as i64 >> i == 1
    }

    // If the permission at index i is checked out, we check the storage does not contain the permission.
    // If the permission is not checked out, we check the storage contains the permission and the permission has the correct cellid.
    // TODO: comment back in checks on is_inserted
    pub open spec fn valid_storage_at_idx(&self, i: nat) -> bool {
        if self.is_checked_out(i) {
            !self.storage.dom().contains(i)
        } else {
            self.storage.dom().contains(i)
            && self.storage.index(i)@.pcell === self.backing_cells.index(i as int)
            // && if self.is_inserted(i) {
            //     self.storage.index(i)@.value.is_Some()
            // } else {
            //     self.storage.index(i)@.value.is_None()
            // }
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
        // Check out the permission for the cell at index i by removing from the storage map and update the bitmap
        check_out_perm(i: nat) {
            // assert(0 <= i && i < pre.backing_cells.len());
            let checked_out_bitmap = pre.checked_out_bitmap;

            withdraw storage -= [i => let perm] by {
                assert(pre.valid_storage_at_idx(i));
            };
            update checked_out_bitmap = (checked_out_bitmap as u64 + ((1 << i) as u64)) as nat;

            assert(
                perm@.pcell === pre.backing_cells.index(i as int)
            ) by {
                assert(pre.valid_storage_at_idx(i));
            };
        }
    }

    transition!{
        // Return the permission for the cell at index i by putting in the storage map and update the bitmap
        return_perm(i: nat, perm: cell::PointsTo<T>) {
            assert(0 <= i && i < pre.backing_cells.len());
            require(perm@.pcell === pre.backing_cells.index(i as int));
            
            let checked_out_bitmap = pre.checked_out_bitmap;

            deposit storage += [i => perm] by { assert(pre.valid_storage_at_idx(i)); };

            update checked_out_bitmap = (checked_out_bitmap as u64 & (!(1u64 << i as u64) as u64)) as nat;
        }
    }

    // TODO: don't know what the following inductives are for, so I copied over some code
    #[inductive(initialize)]
    fn initialize_inductive(post: Self, backing_cells: Seq<CellId>, storage: Map<nat, cell::PointsTo<T>>) {
        assert forall|i: nat|
            0 <= i && i < post.len() implies post.valid_storage_at_idx(i)
        by {
            assert(post.storage.dom().contains(i));
        }
    }

    #[inductive(check_out_perm)]
    fn check_out_perm_inductive(pre: Self, post: Self, i: nat) {
        assert(pre.storage.dom().contains(i));
        assert(
            pre.storage.index(i)@.pcell ===
            pre.backing_cells.index(i as int)
        );

        assert forall |n|
            pre.valid_storage_at_idx(n) implies post.valid_storage_at_idx(n)
        by { }
    }

    #[inductive(return_perm)]
    fn return_perm_inductive(pre: Self, post: Self, i: nat, perm: cell::PointsTo<T>) {
        assert(post.storage.dom().contains(i));
        assert(
                post.storage.index(i)@.pcell ===
                post.backing_cells.index(i as int)
            );

        assert forall |i|
            pre.valid_storage_at_idx(i) implies post.valid_storage_at_idx(i)
        by { }
    }
}}

pub const MAX_RELOCS: usize = 8;

#[derive(Clone, Debug, Copy)]
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
    pub struct HashTable<K, V> {
        // A Vec of PCells that is the actual store of the hash table
        buffer: Vec<PCell<KeyVal<K, V>>>,
        // The atomic bitmaps shared between threads
        checked_out_bitmap_atomic: AtomicU64<_, CuckooHashTable::checked_out_bitmap<KeyVal<K, V>>, _>,
        inserted_bitmap_atomic: AtomicU64<_, CuckooHashTable::inserted_bitmap<KeyVal<K, V>>, _>,

        // The instance of the state machine
        instance: Tracked<CuckooHashTable::Instance<KeyVal<K, V>>>,
    }

    pub closed spec fn wf(&self) -> bool {
        predicate {
            &&& self.instance@.backing_cells().len() == self.buffer@.len()
            &&& forall|i: int| 0 <= i && i < self.buffer@.len() as int ==>
                self.instance@.backing_cells().index(i) ===
                    self.buffer@.index(i).id()
        }

        invariant on checked_out_bitmap_atomic with (instance) is (v: u64, g: CuckooHashTable::checked_out_bitmap<KeyVal<K, V>>) {
            &&& g@.instance === instance@
            &&& g@.value == v as int
        }

        // invariant on inserted_bitmap_atomic with (instance) is (v: u64, g: CuckooHashTable::inserted_bitmap<KeyVal<K, V>>) {
        //     &&& g@.instance === instance@
        //     &&& g@.value == v as int
        // }
    }
}

pub fn new_ht<K, V>(len: usize) -> HashTable<K, V> {
    // Init the back_cells_vec to be stored in the state machine
    let mut backing_cells_vec = Vec::<PCell<KeyVal<K, V>>>::new();

    // Init the storage map to be stored in the state machine
    let tracked mut perms = Map::<nat, cell::PointsTo<KeyVal<K, V>>>::tracked_empty();
    // Make one cell at each loop
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

    // Pass the backing_cells_ids and perms prepared above into the initialize function of the state machine
    let tracked (
        Tracked(instance),
        Tracked(checked_out_bitmap_token),
        Tracked(inserted_bitmap_token),
    ) = CuckooHashTable::Instance::initialize(backing_cells_ids, perms, perms);
    
    // Use the return value of the initialize function to construct the instance and the bitmaps
    let tracked_inst: Tracked<CuckooHashTable::Instance<KeyVal<K, V>>> = Tracked(instance.clone());
    let checked_out_bitmap_atomic = AtomicU64::new(Ghost(tracked_inst), 0, Tracked(checked_out_bitmap_token));
    let inserted_bitmap_atomic = AtomicU64::new(Ghost(tracked_inst), 0, Tracked(inserted_bitmap_token));

    let ht = HashTable::<K, V> {
        instance: Tracked(instance),
        buffer: backing_cells_vec,
        checked_out_bitmap_atomic: checked_out_bitmap_atomic,
        inserted_bitmap_atomic: inserted_bitmap_atomic,
    };

    ht
}

impl<K: std::clone::Clone, V> HashTable<K, V> {
    fn hash1(&self, key: K) -> usize {
        // TODO: temporary solution for not being able to import the hash library
        1
    }

    fn hash2(&self, key: K) -> usize {
        3
    }

    // Find a valid path using BFS so we are guarenteed to find the shortest path.
    // We put the key and subsequently move the keys following the path in the insert function.
    pub fn insert_find_path(&mut self, key: K) -> (p: Option<Vec<usize>>)
        requires
            old(self).wf(),
        ensures
            self.wf(),
            // p == None || (p == Some(res) && res.len() <= MAX_RELOCS),
            match p {
                None => true,
                Some(res) => res.len() <= MAX_RELOCS,
            }
    {
        let mut queue = vec![vec![self.hash1(key.clone())], vec![self.hash2(key.clone())]];

        while queue.len() > 0
            invariant
                self.wf(),
                queue.len() >= 0,
            ensures
                self.wf(),
                queue.len() >= 0,
        {
            if let Some(path) = queue.pop() {
                if path.len() > MAX_RELOCS {
                    break;
                }
    
                let last_idx = path[path.len() - 1];
                let tracked cell_perm: cell::PointsTo<KeyVal<K, V>>;

                // Check out the permission for the last_idx in order to view the key and value in there
                atomic_with_ghost!(&self.checked_out_bitmap_atomic => fetch_or(((1 << last_idx) as u64)); ghost checked_out_bitmap_token => {
                    cell_perm = self.instance.borrow().check_out_perm(last_idx as nat, &mut checked_out_bitmap_token);
                });
    
                let entry = self.buffer[last_idx].borrow(Tracked(&cell_perm));
                if entry.key.is_none() {
                    // We found a valid path so return the permission and return early.
                    atomic_with_ghost!(&self.checked_out_bitmap_atomic => fetch_and((!(1u64 << last_idx as u64) as u64)); ghost checked_out_bitmap_token => {
                        self.instance.borrow().return_perm(last_idx as nat, cell_perm, cell_perm, &mut checked_out_bitmap_token);
                    });
                    return Some(path);
                }
                
                // The current index is occupied, so we need to move the key to its alternative index
                let k = entry.key.as_ref().unwrap();
                let next_idx = if self.hash1(k.clone()) != last_idx {
                    self.hash1(k.clone())
                } else {
                    self.hash2(k.clone())
                };
    
                let mut new_path = path;
                new_path.push(next_idx);
                queue.push(new_path);
                
                // Return the permission before the next iteration
                atomic_with_ghost!(&self.checked_out_bitmap_atomic => fetch_and((!(1u64 << last_idx as u64) as u64)); ghost checked_out_bitmap_token => {
                    self.instance.borrow().return_perm(last_idx as nat, cell_perm, cell_perm, &mut checked_out_bitmap_token);
                });
            } else {
                return None;
            }
        }

        None
    }
}


fn main() {
    let mut ht = new_ht::<String, String>(32);
    let path = ht.insert_find_path("key1".to_string());
    print_u64(path.unwrap()[0] as u64);
}
}