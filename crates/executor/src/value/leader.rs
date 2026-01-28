use crate::value::{RcValue};

pub struct Leader {
    last_modified_stack: Option<u64>,
    leader: RcValue,
    follower: RcValue
}
