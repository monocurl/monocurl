use crate::{heap::VRc, state::LeaderKind};

/// a leader-follower pair for mesh/param variables.
/// the leader is the "code" value that the user modifies.
/// the follower is the "on-screen" value that is interpolated during animations.
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
    // if true, then won't assign to this guy directly
    pub cloned: bool
}

impl Clone for Leader {
    fn clone(&self) -> Self {
        Self {
            kind: self.kind.clone(), last_modified_stack: self.last_modified_stack.clone(),
            locked_by_anim: self.locked_by_anim.clone(),
            leader_rc: self.leader_rc.clone(),
            leader_version: self.leader_version.clone(), follower_rc: self.follower_rc.clone(), follower_version: self.follower_version.clone(),
            cloned: true
        }
    }
}
