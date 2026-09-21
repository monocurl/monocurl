use std::any::Any;

use crate::{
    error::{Error, Result},
    keyed::Keyed,
    mesh::{Leaf, MeshValue, Shape},
    operator::{ConstructorNode, OpId, Slot},
};

/// Retained constructor node for `circle(radius)`. Hand-written against the
/// raw `ConstructorNode` trait so the built-in doesn't have to wait on the
/// `#[constructor]` macro (milestone 4) to exist — the acceptance test at the
/// bottom of `OPERATORS.md` needs a real, identity-retaining `circle` from
/// milestone 1 onward, since it exercises structural lerp rule 3/4 through
/// it. `#[constructor]`, once it lands, generates exactly this shape.
#[derive(Clone, Debug)]
struct CircleNode {
    radius: f64,
}

impl ConstructorNode for CircleNode {
    fn op_id(&self) -> OpId {
        OpId::of::<CircleNode>()
    }

    fn evaluate(&self) -> MeshValue {
        MeshValue::leaf(Leaf::new(Shape::Circle {
            radius: self.radius,
        }))
    }

    fn lerp_slots(&self, other: &dyn ConstructorNode, t: f64) -> Result<Box<dyn ConstructorNode>> {
        let other = other.as_any().downcast_ref::<CircleNode>().ok_or_else(|| {
            Error::message("cannot lerp `circle`: operand has a different constructor identity")
        })?;
        Ok(Box::new(CircleNode {
            radius: self.radius.lerp(&other.radius, t)?,
        }))
    }

    fn slot(&self, index: usize) -> &dyn Slot {
        match index {
            0 => &self.radius,
            _ => panic!("slot index {index} out of range for `circle` (1 slot)"),
        }
    }

    fn slot_mut(&mut self, index: usize) -> &mut dyn Slot {
        match index {
            0 => &mut self.radius,
            _ => panic!("slot index {index} out of range for `circle` (1 slot)"),
        }
    }

    fn slot_count(&self) -> usize {
        1
    }

    fn clone_node(&self) -> Box<dyn ConstructorNode> {
        Box::new(self.clone())
    }

    fn debug_name(&self) -> &'static str {
        "circle"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// `circle(0.35)` — a retained, chainable, lerpable built-in constructor.
pub fn circle(radius: f64) -> MeshValue {
    MeshValue::from_constructor_node(Box::new(CircleNode { radius }))
}
