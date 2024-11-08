use vstd::atomic_ghost::*;
use vstd::cell::*;
use vstd::modes::*;
use vstd::prelude::*;
use vstd::{pervasive::*, *};

verus! {
struct_with_invariants!{
    struct Lock<T> {
        // The type placeholders are filled in by the
        // struct_with_invariants! macro.
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
    let lock = Lock::new(10 as u64);
    let mut cell_perm = lock.acquire();
    print_u64(lock.cell.take(Tracked(cell_perm.borrow_mut())));
    lock.cell.put(Tracked(cell_perm.borrow_mut()), 20);
    print_u64(*lock.cell.borrow(Tracked(cell_perm.borrow())));
    lock.release(cell_perm);
}
}