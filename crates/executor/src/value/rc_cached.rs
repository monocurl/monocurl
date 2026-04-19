use std::rc::Rc;

/// value type that shares an immutable `Body` via `Rc` while keeping
/// a `Cache` independently owned per value instance (not shared).
/// cloning is cheap for the body (Rc pointer copy) and independent for the cache.
pub struct RcCached<Body, Cache> {
    pub body: Rc<Body>,
    pub cache: Cache,
}

impl<Body, Cache: Clone> Clone for RcCached<Body, Cache> {
    fn clone(&self) -> Self {
        Self {
            body: Rc::clone(&self.body),
            cache: self.cache.clone(),
        }
    }
}
