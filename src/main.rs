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

verus! {
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

impl<T> Lock<T> {
    fn new(t: T) -> (lock: Self)
        ensures lock.wf()
    {
        let (cell, Tracked(cell_perm)) = PCell::new(t);
        let atomic = AtomicBool::new(Ghost(cell), false, Tracked(Some(cell_perm)));
        Lock { atomic, cell }
    }

    fn acquire(&self) -> (points_to: Tracked<cell::PointsTo<T>>)
        requires self.wf(),
        ensures points_to@.id() == self.cell.id(), points_to@.is_init()
    {
        loop
            invariant self.wf(),
        {
            let tracked mut points_to_opt = None;
            let res = atomic_with_ghost!(&self.atomic => compare_exchange(false, true);
                ghost g => {
                    tracked_swap(&mut points_to_opt, &mut g);
                }
            );
            if res.is_ok() {
                return Tracked(points_to_opt.tracked_unwrap());
            }
        }
    }

    fn release(&self, Tracked(points_to): Tracked<cell::PointsTo<T>>)
        requires self.wf(), points_to.id() == self.cell.id(), points_to.is_init()
    {
        atomic_with_ghost!(&self.atomic => store(false);
            ghost g => {
                g = Some(points_to);
            }
        );
    }
}

fn main() {
    // let lock = Lock::new(10 as u64);
    // let mut cell_perm = lock.acquire();
    // print_u64(lock.cell.take(Tracked(cell_perm.borrow_mut())));
    // lock.cell.put(Tracked(cell_perm.borrow_mut()), 20);
    // print_u64(*lock.cell.borrow(Tracked(cell_perm.borrow())));
    // lock.release(cell_perm);

    let lock = Arc::new(Lock::new(0 as u64));
    let mut handles = Vec::new();
    let mut i = 0;
    while i < 13
        invariant
            (*lock).wf(),
    {
        let l = Arc::clone(&lock);

        let handle = spawn(move ||
        {
            let mut cell_perm = (*l).acquire();
            let mut prev = (*l).cell.take(Tracked(cell_perm.borrow_mut()));
            if prev == u64::MAX {
                prev = u64::MAX - 1
            }
            (*l).cell.put(Tracked(cell_perm.borrow_mut()), prev + 1);
            (*l).release(cell_perm);
        });

        i = i + 1;
        handles.push(handle);
    }

    i = 0;
    while i < 13 {
        match handles.pop() {
            Some(handle) => {
                match handle.join() {
                    Result::Ok(prev) => {
                    },
                    _ => {
                        return;
                    },
                };
            },
            None => {
                return;
            },
        }
        i = i + 1;
    }

    let mut cell_perm = (*lock).acquire();
    print_u64(*lock.cell.borrow(Tracked(cell_perm.borrow())));
    (*lock).release(cell_perm);
}
}