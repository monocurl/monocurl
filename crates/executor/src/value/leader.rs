use crate::{heap::VRc, state::LeaderKind};

/// a leader-follower pair for mesh/scene variables.
/// the leader is the "code" value that the user modifies.
/// the follower is the "on-screen" value that is interpolated during animations.
#[derive(Clone)]
pub struct Leader {
    pub kind: LeaderKind,
    pub last_modified_stack: Option<usize>,
    pub locked_by_anim: Option<usize>,
    /// heap slot containing the leader value (what code sees)
    pub leader_rc: VRc,
    pub leader_version: u64,
    /// heap slot containing the follower value (what's on screen)
    pub follower_rc: VRc,
    pub follower_version: u64,
}
