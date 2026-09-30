use std::{
    cell::RefCell,
    ops::{Deref, DerefMut},
    rc::Rc,
};

use crate::{
    heap::{refcounting_inhibited, snapshot_copy},
    value::Value,
};

/// a live wrapper's cached result. copies of the wrapper share it, so it is
/// read in place rather than taken out and put back: two readers of one cell
/// (`x == x`) must both see the value
#[derive(Default)]
pub struct CacheCell(RefCell<Option<Box<Value>>>);

impl CacheCell {
    pub fn new(value: Option<Value>) -> Self {
        Self(RefCell::new(value.map(Box::new)))
    }

    pub fn fill(&self, value: Value) {
        *self.0.borrow_mut() = Some(Box::new(value));
    }

    pub fn take(&self) -> Option<Box<Value>> {
        self.0.borrow_mut().take()
    }

    pub fn is_filled(&self) -> bool {
        self.0.borrow().is_some()
    }

    pub fn cloned(&self) -> Option<Value> {
        self.0.borrow().as_deref().cloned()
    }

    /// inspect the cached value without copying it; `inspect` must not fill or
    /// take this cell
    pub fn with<R>(&self, inspect: impl FnOnce(Option<&Value>) -> R) -> R {
        inspect(self.0.borrow().as_deref())
    }
}

impl Clone for CacheCell {
    fn clone(&self) -> Self {
        Self(RefCell::new(self.0.borrow().clone()))
    }
}

/// a live value shared between its copies. copying a value is far more common
/// than editing one, so a copy is a reference count away and the first edit
/// through any copy detaches it (`DerefMut`). the cache is shared too: it
/// depends only on the body, which no copy can change without detaching.
///
/// a heap snapshot is the exception: it must not share with the live heap,
/// whose caches keep filling with values the snapshot's slots know nothing
/// about. a copy taken while refcounting is inhibited is therefore a deep one,
/// shared only with the other copies the same snapshot makes of this value
pub struct RcCached<Body, Cache>(Rc<RcCachedBody<Body, Cache>>);

impl<Body: Clone + 'static, Cache: Clone + 'static> Clone for RcCached<Body, Cache> {
    fn clone(&self) -> Self {
        if refcounting_inhibited() {
            Self(snapshot_copy(Rc::as_ptr(&self.0), || {
                Rc::new((*self.0).clone())
            }))
        } else {
            Self(Rc::clone(&self.0))
        }
    }
}

#[derive(Clone)]
pub struct RcCachedBody<Body, Cache> {
    pub body: Body,
    pub cache: Cache,
}

impl<Body, Cache> RcCached<Body, Cache> {
    pub fn new(body: Body, cache: Cache) -> Self {
        Self(Rc::new(RcCachedBody { body, cache }))
    }
}

impl<Body, Cache> Deref for RcCached<Body, Cache> {
    type Target = RcCachedBody<Body, Cache>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<Body: Clone, Cache: Clone> DerefMut for RcCached<Body, Cache> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Rc::make_mut(&mut self.0)
    }
}
