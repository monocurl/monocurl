#[derive(Clone)]
pub struct RcCached<Body, Cache> {
    pub body: Body,
    pub cache: Cache,
}
