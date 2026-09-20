use std::{
    cell::{Cell, Ref, RefCell, RefMut},
    mem::ManuallyDrop,
};

use crate::value::Value;

pub type HeapKey = u32;

/// the heap and its inhibit flag share one thread-local so that a refcount
/// change costs a single TLS lookup instead of two; these are the hottest
/// operations in the interpreter
struct HeapCell {
    heap: RefCell<VirtualHeap>,
    /// when set, VRc::clone and VRc::drop skip refcount changes.
    /// also checked by heap_release to prevent snapshot drops from corrupting live heap.
    inhibit: Cell<bool>,
}

thread_local! {
    // const-initialized so access skips the lazy-initialization check; the
    // interpreter touches this on essentially every value read
    static HEAP: HeapCell = const {
        HeapCell {
            heap: RefCell::new(VirtualHeap::new()),
            inhibit: Cell::new(false),
        }
    };
}

fn refcount_inhibited() -> bool {
    HEAP.try_with(|cell| cell.inhibit.get()).unwrap_or(true)
}

pub struct VirtualHeap {
    slots: Vec<RefCell<Option<Value>>>,
    ref_counts: Vec<Cell<u32>>,
    free_list: RefCell<Vec<HeapKey>>,
}

pub struct RawHeapSnapshot<T>(ManuallyDrop<T>);

impl<T> AsRef<T> for RawHeapSnapshot<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T: Clone> RawHeapSnapshot<T> {
    pub fn new(value: &T) -> Self {
        with_inhibit(|| Self(ManuallyDrop::new(value.clone())))
    }

    pub fn raw_clone(&self) -> T {
        with_inhibit(|| (*self.0).clone())
    }
}

impl<T: Clone> Clone for RawHeapSnapshot<T> {
    fn clone(&self) -> Self {
        Self::new(self.as_ref())
    }
}

impl<T> Drop for RawHeapSnapshot<T> {
    fn drop(&mut self) {
        with_inhibit(|| unsafe {
            ManuallyDrop::drop(&mut self.0);
        });
    }
}

impl VirtualHeap {
    const fn new() -> Self {
        Self {
            slots: Vec::new(),
            ref_counts: Vec::new(),
            free_list: RefCell::new(Vec::new()),
        }
    }

    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    #[cfg(any(test, feature = "test_heap_tracking"))]
    pub fn live_slot_count(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.borrow().is_some())
            .count()
    }

    #[cfg(any(test, feature = "test_heap_tracking"))]
    pub fn free_slot_count(&self) -> usize {
        self.slot_count() - self.live_slot_count()
    }

    pub fn get(&self, key: HeapKey) -> Ref<'_, Value> {
        Ref::map(self.slots[key as usize].borrow(), |opt| {
            opt.as_ref().expect("HeapKey points to free slot")
        })
    }

    pub fn get_mut(&self, key: HeapKey) -> RefMut<'_, Value> {
        RefMut::map(self.slots[key as usize].borrow_mut(), |opt| {
            opt.as_mut().expect("HeapKey points to free slot")
        })
    }
}

impl Clone for VirtualHeap {
    fn clone(&self) -> Self {
        Self {
            slots: self
                .slots
                .iter()
                .map(|cell| RefCell::new(cell.borrow().clone()))
                .collect(),
            ref_counts: self.ref_counts.iter().map(|c| Cell::new(c.get())).collect(),
            free_list: RefCell::new(self.free_list.borrow().clone()),
        }
    }
}

/// owning reference to a heap slot; Clone retains, Drop releases (both inhibitable)
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct VRc(HeapKey);

/// non-owning reference; Copy, never changes refcount
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct VWeak(pub HeapKey);

impl Clone for VRc {
    fn clone(&self) -> Self {
        let _ = HEAP.try_with(|cell| retain_in(cell, self.0));
        VRc(self.0)
    }
}

#[inline]
fn retain_in(cell: &HeapCell, key: HeapKey) {
    if cell.inhibit.get() {
        return;
    }
    let heap = cell.heap.borrow();
    let count = &heap.ref_counts[key as usize];
    count.set(count.get() + 1);
}

impl Drop for VRc {
    fn drop(&mut self) {
        if !refcount_inhibited() {
            heap_release(self.0);
        }
    }
}

impl VRc {
    pub fn new(val: Value) -> Self {
        VRc(heap_alloc(val))
    }

    pub fn key(&self) -> HeapKey {
        self.0
    }

    pub fn downgrade(&self) -> VWeak {
        VWeak(self.0)
    }

    /// wrap a key that was just allocated (refcount already=1) or already retained
    pub fn from_retained(key: HeapKey) -> Self {
        VRc(key)
    }

    /// retain the slot and wrap; equivalent to VRc::new but for an existing slot
    pub fn retain_key(key: HeapKey) -> Self {
        heap_retain(key);
        VRc(key)
    }

    pub fn make_mut(&mut self) -> HeapKey {
        if heap_ref_count(self.0) > 1 {
            let value = with_heap(|h| h.get(self.0).clone());
            *self = VRc::new(value);
        }
        self.0
    }
}

impl VWeak {
    pub fn key(&self) -> HeapKey {
        self.0
    }

    /// create an owning VRc by incrementing refcount
    pub fn upgrade(&self) -> VRc {
        VRc::retain_key(self.0)
    }
}

pub fn heap_alloc(val: Value) -> HeapKey {
    HEAP.with(|cell| {
        let mut heap = cell.heap.borrow_mut();
        if let Some(key) = heap.free_list.get_mut().pop() {
            *heap.slots[key as usize].get_mut() = Some(val);
            heap.ref_counts[key as usize].set(1);
            key
        } else {
            let idx = heap.slots.len() as u32;
            heap.slots.push(RefCell::new(Some(val)));
            heap.ref_counts.push(Cell::new(1));
            idx
        }
    })
}

/// increment refcount; skipped during snapshot/restore
pub fn heap_retain(key: HeapKey) {
    let _ = HEAP.try_with(|cell| retain_in(cell, key));
}

/// decrement refcount; free slot if it hits zero.
/// no-op while refcounting is inhibited (prevents snapshot drops from corrupting the live heap).
pub fn heap_release(key: HeapKey) {
    let Ok(should_free) = HEAP.try_with(|cell| {
        if cell.inhibit.get() {
            return false;
        }
        let heap = cell.heap.borrow();
        let count = &heap.ref_counts[key as usize];
        let old = count.get();
        debug_assert!(old > 0, "heap_release on already-freed HeapKey {}", key);
        count.set(old - 1);
        old == 1
    }) else {
        return;
    };
    if should_free {
        // use shared borrow + per-slot borrow_mut; safe for reentry from nested heap_releases
        let Ok(val) = HEAP.try_with(|cell| {
            let heap = cell.heap.borrow();
            heap.free_list.borrow_mut().push(key);
            heap.slots[key as usize].borrow_mut().take().unwrap()
        }) else {
            return;
        };
        // val dropped after borrow released; nested heap_releases are fine
        drop(val);
    }
}

/// swap the slot value safely: borrow released before dropping the old value
pub fn heap_replace(key: HeapKey, new_val: Value) {
    let old = HEAP.with(|cell| {
        let heap = cell.heap.borrow();
        heap.slots[key as usize].borrow_mut().replace(new_val)
    });
    drop(old);
}

pub fn heap_ref_count(key: HeapKey) -> u32 {
    HEAP.with(|cell| cell.heap.borrow().ref_counts[key as usize].get())
}

pub fn with_heap<R>(f: impl FnOnce(&VirtualHeap) -> R) -> R {
    HEAP.with(|cell| f(&cell.heap.borrow()))
}

/// same as with_heap but signals intent to mutate via per-slot RefCells
pub fn with_heap_mut<R>(f: impl FnOnce(&VirtualHeap) -> R) -> R {
    HEAP.with(|cell| f(&cell.heap.borrow()))
}

/// snapshot the heap; INHIBIT prevents VRc refcount side-effects during clone
pub fn snapshot_heap() -> RawHeapSnapshot<VirtualHeap> {
    HEAP.with(|cell| RawHeapSnapshot::new(&*cell.heap.borrow()))
}

/// restore the heap from a snapshot; INHIBIT prevents the old heap's drop
/// from corrupting the newly-installed heap's refcounts
pub fn restore_heap(snap: &RawHeapSnapshot<VirtualHeap>) {
    with_inhibit(|| {
        let new_heap = snap.raw_clone();
        let _old = HEAP.with(|cell| std::mem::replace(&mut *cell.heap.borrow_mut(), new_heap));
        drop(_old);
    });
}

/// clone a value without refcount side effects (for raw state snapshot/restore)
pub fn raw_clone<T: Clone>(val: &T) -> T {
    with_inhibit(|| val.clone())
}

/// run `f` with refcount inhibited; restores prior inhibit state on return.
/// use this when dropping values that were snapshotted without refcount tracking.
pub fn with_inhibit<R>(f: impl FnOnce() -> R) -> R {
    let prev = HEAP.try_with(|cell| {
        let prev = cell.inhibit.get();
        cell.inhibit.set(true);
        prev
    });
    let r = f();
    if let Ok(prev) = prev {
        let _ = HEAP.try_with(|cell| cell.inhibit.set(prev));
    }
    r
}

#[cfg(test)]
mod tests {
    use super::{VRc, heap_ref_count, snapshot_heap, with_heap};
    use crate::value::{Value, container::List};

    #[test]
    fn dropping_vrc_releases_nested_heap_slots() {
        let baseline = with_heap(|heap| heap.live_slot_count());

        {
            let _value = VRc::new(Value::List(List::new_with(vec![
                VRc::new(Value::Integer(1)),
                VRc::new(Value::List(List::new_with(vec![
                    VRc::new(Value::Integer(2)),
                    VRc::new(Value::Integer(3)),
                ]))),
            ])));

            assert_eq!(with_heap(|heap| heap.live_slot_count()), baseline + 5);
        }

        assert_eq!(with_heap(|heap| heap.live_slot_count()), baseline);
    }

    #[test]
    fn freed_heap_slots_are_reused() {
        let baseline_live = with_heap(|heap| heap.live_slot_count());

        {
            let _values = (0..32)
                .map(|value| VRc::new(Value::Integer(value)))
                .collect::<Vec<_>>();
            assert_eq!(with_heap(|heap| heap.live_slot_count()), baseline_live + 32);
        }

        assert_eq!(with_heap(|heap| heap.live_slot_count()), baseline_live);
        let high_water_mark = with_heap(|heap| heap.slot_count());

        {
            let _values = (0..32)
                .map(|value| VRc::new(Value::Integer(value)))
                .collect::<Vec<_>>();
        }

        assert_eq!(with_heap(|heap| heap.live_slot_count()), baseline_live);
        assert_eq!(with_heap(|heap| heap.slot_count()), high_water_mark);
    }

    #[test]
    fn dropping_raw_heap_snapshot_does_not_release_live_slots() {
        let baseline = with_heap(|heap| heap.live_slot_count());
        let value = VRc::new(Value::Integer(1));
        let key = value.key();

        let snapshot = snapshot_heap();
        drop(snapshot);

        assert_eq!(heap_ref_count(key), 1);
        drop(value);
        assert_eq!(with_heap(|heap| heap.live_slot_count()), baseline);
    }
}
