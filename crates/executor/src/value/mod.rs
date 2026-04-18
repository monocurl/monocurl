mod equality;
mod helpers;

pub mod anim_block;
pub mod container;
pub mod invoked_function;
pub mod invoked_operator;
pub mod lambda;
pub mod leader;
pub mod primitive_anim;
pub mod stateful;

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use geo::mesh::Mesh;

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

/// owning reference to a mutable value cell.
/// containers (List, Map) and promoted variables hold these.
pub type RcValue = Rc<RefCell<Value>>;

/// non-owning reference used for pushed lvalue refs to break reference cycles.
pub type WeakValue = Weak<RefCell<Value>>;

/// create a new RcValue wrapping the given value
pub fn rc_value(val: Value) -> RcValue {
    Rc::new(RefCell::new(val))
}

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

    InvokedOperator(Rc<InvokedOperator>),
    InvokedFunction(Rc<InvokedFunction>),

    /// owning lvalue — the strong Rc lives on the var_stack at the promoted slot.
    Lvalue(RcValue),
    /// non-owning lvalue reference — pushed via PushLvalue.
    /// upgrading can fail if the owning variable was freed.
    WeakLvalue(WeakValue),
}
