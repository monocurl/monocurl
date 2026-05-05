use std::ops::{Deref, DerefMut};

#[derive(Clone)]
pub struct RcCached<Body, Cache>(Box<RcCachedBody<Body, Cache>>);

#[derive(Clone)]
pub struct RcCachedBody<Body, Cache> {
    pub body: Body,
    pub cache: Cache,
}

impl<Body, Cache> RcCached<Body, Cache> {
    pub fn new(body: Body, cache: Cache) -> Self {
        Self(Box::new(RcCachedBody { body, cache }))
    }
}

impl<Body, Cache> Deref for RcCached<Body, Cache> {
    type Target = RcCachedBody<Body, Cache>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<Body, Cache> DerefMut for RcCached<Body, Cache> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
