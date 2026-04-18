use crate::state::LeaderKind;

use super::RcValue;

/// a leader-follower pair for mesh/param variables.
/// the leader is the "code" value that the user modifies.
/// the follower is the "on-screen" value that is interpolated during animations.
#[derive(Clone)]
pub struct Leader {
    /// whether this is a mesh or param leader
    pub kind: LeaderKind,
    /// the stack id of the last execution stack that modified this leader
    pub last_modified_stack: Option<usize>,
    /// active primitive animation currently owning this leader/follower pair
    pub locked_by_anim: Option<usize>,
    /// the leader value (what code sees)
    pub leader_rc: RcValue,
    /// the follower value (what's on screen)
    pub follower_rc: RcValue,
    /// incremented each time follower_rc is written; used for stateful cache invalidation
    pub follower_version: u64,
}
