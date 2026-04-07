use crate::value::RcValue;

pub type VHeapPtr = usize;

const SENTINEL: usize = usize::MAX;

#[derive(Clone, Copy)]
struct FreeNode {
    prev: usize,
    next: usize,
}

pub struct VHeap<const N: usize = { 1 << 20 }> {
    data: Vec<Option<RcValue>>,
    free_nodes: Vec<FreeNode>,
    free_head: usize,
}

impl<const N: usize> VHeap<N> {
    pub fn new() -> Self {
        let free_nodes = (0..N)
            .map(|i| FreeNode {
                prev: if i == 0 { SENTINEL } else { i - 1 },
                next: if i + 1 == N { SENTINEL } else { i + 1 },
            })
            .collect();

        Self {
            data: (0..N).map(|_| None).collect(),
            free_nodes,
            free_head: if N > 0 { 0 } else { SENTINEL },
        }
    }

    pub fn alloc(&mut self) -> Result<VHeapPtr, ()> {
        if self.free_head == SENTINEL {
            return Err(());
        }
        let ptr = self.free_head;
        let next = self.free_nodes[ptr].next;
        if next != SENTINEL {
            self.free_nodes[next].prev = SENTINEL;
        }
        self.free_head = next;
        Ok(ptr)
    }

    pub fn dealloc(&mut self, ptr: VHeapPtr) {
        self.data[ptr] = None;
        let old_head = self.free_head;
        self.free_nodes[ptr] = FreeNode { prev: SENTINEL, next: old_head };
        if old_head != SENTINEL {
            self.free_nodes[old_head].prev = ptr;
        }
        self.free_head = ptr;
    }

    pub fn write(&mut self, at: VHeapPtr, val: RcValue) {
        self.data[at] = Some(val);
    }

    pub fn read(&self, at: VHeapPtr) -> RcValue {
        self.data[at].as_ref().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;
    use crate::value::{Value, primitive::IntPrimitive};

    fn make_val(n: i64) -> RcValue {
        Rc::new(Cell::new(Value::Integer(IntPrimitive(n))))
    }

    fn read_int<const N: usize>(heap: &VHeap<N>, ptr: VHeapPtr) -> i64 {
        let rc = heap.read(ptr);
        let v = rc.replace(Value::Integer(IntPrimitive(0)));
        let Value::Integer(IntPrimitive(n)) = v else { panic!("not int") };
        rc.set(Value::Integer(IntPrimitive(n)));
        n
    }

    #[test]
    fn alloc_and_read() {
        let mut heap = VHeap::<16>::new();
        let ptr = heap.alloc().unwrap();
        heap.write(ptr, make_val(42));
        assert_eq!(read_int(&heap, ptr), 42);
    }

    #[test]
    fn dealloc_then_realloc() {
        let mut heap = VHeap::<16>::new();
        let a = heap.alloc().unwrap();
        heap.write(a, make_val(1));
        heap.dealloc(a);
        // next alloc should reuse the freed slot
        let b = heap.alloc().unwrap();
        assert_eq!(b, a);
        heap.write(b, make_val(99));
        assert_eq!(read_int(&heap, b), 99);
    }

    #[test]
    fn fill_to_capacity() {
        let mut heap = VHeap::<4>::new();
        let ptrs: Vec<_> = (0..4).map(|i| {
            let p = heap.alloc().unwrap();
            heap.write(p, make_val(i));
            p
        }).collect();
        assert!(heap.alloc().is_err());
        // values intact
        for (i, &p) in ptrs.iter().enumerate() {
            assert_eq!(read_int(&heap, p), i as i64);
        }
    }

    #[test]
    fn oom_returns_err() {
        let mut heap = VHeap::<2>::new();
        heap.alloc().unwrap();
        heap.alloc().unwrap();
        assert!(heap.alloc().is_err());
    }

    #[test]
    fn free_restores_capacity() {
        let mut heap = VHeap::<2>::new();
        let a = heap.alloc().unwrap();
        heap.write(a, make_val(10));
        let b = heap.alloc().unwrap();
        heap.write(b, make_val(20));
        assert!(heap.alloc().is_err());

        heap.dealloc(a);
        let c = heap.alloc().unwrap();
        assert_eq!(c, a);
        heap.write(c, make_val(30));
        assert_eq!(read_int(&heap, b), 20);
        assert_eq!(read_int(&heap, c), 30);
    }

    #[test]
    fn multiple_cycles() {
        let mut heap = VHeap::<4>::new();
        for round in 0..100i64 {
            let p = heap.alloc().unwrap();
            heap.write(p, make_val(round));
            assert_eq!(read_int(&heap, p), round);
            heap.dealloc(p);
        }
    }

    #[test]
    fn interleaved_alloc_dealloc() {
        let mut heap = VHeap::<8>::new();
        let a = heap.alloc().unwrap();
        let b = heap.alloc().unwrap();
        let c = heap.alloc().unwrap();
        heap.write(a, make_val(1));
        heap.write(b, make_val(2));
        heap.write(c, make_val(3));

        heap.dealloc(b);

        let d = heap.alloc().unwrap();
        heap.write(d, make_val(4));

        assert_eq!(read_int(&heap, a), 1);
        assert_eq!(read_int(&heap, d), 4);
        assert_eq!(read_int(&heap, c), 3);
    }

    #[test]
    fn zero_capacity() {
        let mut heap = VHeap::<0>::new();
        assert!(heap.alloc().is_err());
    }

    #[test]
    fn drain_and_refill() {
        const CAP: usize = 8;
        let mut heap = VHeap::<CAP>::new();
        let ptrs: Vec<_> = (0..CAP).map(|_| heap.alloc().unwrap()).collect();
        assert!(heap.alloc().is_err());
        for &p in &ptrs {
            heap.dealloc(p);
        }
        // all slots freed; should be able to fill again
        let ptrs2: Vec<_> = (0..CAP).map(|i| {
            let p = heap.alloc().unwrap();
            heap.write(p, make_val(i as i64));
            p
        }).collect();
        assert!(heap.alloc().is_err());
        for (i, &p) in ptrs2.iter().enumerate() {
            assert_eq!(read_int(&heap, p), i as i64);
        }
    }
}
