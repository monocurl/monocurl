mod equality;
mod helpers;

pub mod anim_block;
pub mod container;
pub mod invoked_function;
pub mod invoked_operator;
pub mod lambda;
pub mod leader;
pub mod primitive_anim;
pub mod rc_cached;
pub mod stateful;

use std::rc::Rc;
use std::sync::Arc;

use geo::mesh::Mesh;

use crate::heap::{VRc, VWeak};

use self::{
    anim_block::AnimBlock,
    container::{List, Map},
    invoked_function::InvokedFunction,
    invoked_operator::InvokedOperator,
    lambda::{Lambda, Operator},
    leader::Leader,
    primitive_anim::PrimitiveAnim,
    stateful::Stateful,
};

/// (section_index, instruction_offset)
pub type InstructionPointer = (u16, u32);

#[derive(Clone)]
pub enum Value {
    Nil,
    Float(f64),
    Integer(i64),
    Complex {
        re: f64,
        im: f64,
    },
    String(String),

    Mesh(Arc<Mesh>),
    PrimitiveAnim(PrimitiveAnim),

    Lambda(Rc<Lambda>),
    Operator(Operator),
    AnimBlock(Rc<AnimBlock>),
    Map(Rc<Map>),
    List(Rc<List>),

    Stateful(Stateful),
    Leader(Leader),

    InvokedOperator(InvokedOperator),
    InvokedFunction(InvokedFunction),

    /// owning lvalue — the strong VRc lives on the var_stack at the promoted slot.
    Lvalue(VRc),
    /// non-owning lvalue reference — pushed via PushLvalue.
    WeakLvalue(VWeak),
}
