use super::RcValue;

/// a leader-follower pair for mesh/state/param variables.
/// the leader is the "code" value that the user modifies.
/// the follower is the "on-screen" value that is interpolated during animations.
#[derive(Clone)]
pub struct Leader {
    /// the stack id of the last execution stack that modified this leader
    pub last_modified_stack: Option<usize>,
    /// active primitive animation currently owning this leader/follower pair
    pub locked_by_anim: Option<usize>,
    /// the leader value (what code sees)
    pub leader_rc: RcValue,
    /// the follower value (what's on screen)
    pub follower_rc: RcValue,
}
