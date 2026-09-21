use std::any::Any;

use crate::{
    error::{Error, Result},
    keyed::Keyed,
    mesh::MeshValue,
    operator::{OpId, OperatorNode, Slot},
    vector::Vec3,
};

/// Translates every leaf in the operand by `delta`. The reference operator
/// for milestone 1: hand-written against the raw traits (no macros) to prove
/// the trait layer is usable on its own, and used as a fixture by the lerp
/// tests in `lerp.rs` / `tests/`.
#[derive(Clone, Debug)]
pub struct Shift {
    pub delta: Vec3,
}

impl Shift {
    pub fn new(delta: Vec3) -> Self {
        Self { delta }
    }
}

impl OperatorNode for Shift {
    fn op_id(&self) -> OpId {
        OpId::of::<Shift>()
    }

    fn endpoints(&self, target: MeshValue) -> (MeshValue, MeshValue) {
        // Default identity endpoint: the untouched operand (correct for a
        // displacement operator like this one).
        let modified = target.map_leaves(|leaf| leaf.translated(self.delta));
        (target, modified)
    }

    fn lerp_slots(&self, other: &dyn OperatorNode, t: f64) -> Result<Box<dyn OperatorNode>> {
        let other = other.as_any().downcast_ref::<Shift>().ok_or_else(|| {
            Error::message("cannot lerp `shift`: operand has a different operator identity")
        })?;
        Ok(Box::new(Shift {
            delta: self.delta.lerp(&other.delta, t)?,
        }))
    }

    fn slot(&self, index: usize) -> &dyn Slot {
        match index {
            0 => &self.delta,
            _ => panic!("slot index {index} out of range for `shift` (1 slot)"),
        }
    }

    fn slot_mut(&mut self, index: usize) -> &mut dyn Slot {
        match index {
            0 => &mut self.delta,
            _ => panic!("slot index {index} out of range for `shift` (1 slot)"),
        }
    }

    fn slot_count(&self) -> usize {
        1
    }

    fn clone_node(&self) -> Box<dyn OperatorNode> {
        Box::new(self.clone())
    }

    fn debug_name(&self) -> &'static str {
        "shift"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub trait ShiftChainExt {
    fn shift(self, delta: Vec3) -> MeshValue;
}

impl ShiftChainExt for MeshValue {
    fn shift(self, delta: Vec3) -> MeshValue {
        self.with(Shift::new(delta))
    }
}
