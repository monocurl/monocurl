use std::{cell::{Cell}, rc::Rc};

use crate::{value::{anim_block::AnimBlock, container::{List, Map}, invoked_function::InvokedFunction, invoked_operator::InvokedOperator, lambda::{Lambda, Operator}, leader::Leader, primitive::{FloatPrimitive, IntPrimitive, StringPrimitve}, primitive_anim::PrimitiveAnim, primitive_mesh::PrimitiveMesh, stateful::Stateful}, vheap::VHeapPtr};

pub mod invoked_operator;
pub mod invoked_function;
pub mod primitive_anim;
pub mod primitive_mesh;
pub mod lambda;
pub mod primitive;
pub mod container;
pub mod anim_block;
pub mod stateful;
pub mod leader;

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

    // indexes into virtual heap
    Lvalue(VHeapPtr, VHeapPtr),
    Mesh(VHeapPtr, VHeapPtr),
    State(VHeapPtr, VHeapPtr),
    Param(VHeapPtr, VHeapPtr),
}

pub type RcValue = Rc<Cell<Value>>;
pub type InstructionPointer = (u16, u32); // (section, instruction index)

pub trait ValueTrait {
    fn alu_op(&self);
}
