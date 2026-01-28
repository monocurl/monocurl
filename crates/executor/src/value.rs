use std::{cell::{Cell}, rc::Rc};

use crate::{executor::AllocTracker, value::{anim_block::AnimBlock, container::{List, Map}, invoked_function::InvokedFunction, invoked_operator::InvokedOperator, lambda::{Lambda, Operator}, leader::Leader, primitive::{FloatPrimitive, IntPrimitive, StringPrimitve}, primitive_anim::PrimitiveAnim, primitive_mesh::PrimitiveMesh, stateful::Stateful}};

mod invoked_operator;
mod invoked_function;
mod primitive_anim;
mod primitive_mesh;
mod lambda;
mod primitive;
mod container;
mod anim_block;
mod stateful;
mod leader;

pub enum Value {
    Float(FloatPrimitive),
    Integer(IntPrimitive),
    String(StringPrimitve),

    PrimitiveMesh(PrimitiveMesh),
    PrimitiveAnim(PrimitiveAnim),

    Lambda(Lambda),
    Operator(Operator),
    AnimBlock(AnimBlock),
    Map(Map),
    List(List),

    Stateful(Stateful),

    Leader(Leader),

    InvokedOperator(InvokedOperator),
    InvokedFunction(InvokedFunction),
}

pub type RcValue = Rc<(AllocTracker, Cell<Value>)>;
pub type InstructionPointer = (u16, u32); // (section, instruction index)

pub trait ValueTrait {
    fn alu_op(&self);
}
