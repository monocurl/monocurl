pub mod anim_block;
pub mod container;
pub mod invoked_function;
pub mod invoked_operator;
pub mod lambda;
pub mod leader;
pub mod primitive_anim;
pub mod primitive_mesh;
pub mod stateful;

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use self::{
    anim_block::AnimBlock,
    container::{List, Map},
    invoked_function::InvokedFunction,
    invoked_operator::InvokedOperator,
    lambda::{Lambda, Operator},
    leader::Leader,
    primitive_anim::PrimitiveAnim,
    primitive_mesh::PrimitiveMesh,
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
    Complex { re: f64, im: f64 },
    String(String),

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

    /// owning lvalue — the strong Rc lives on the var_stack at the promoted slot.
    Lvalue(RcValue),
    /// non-owning lvalue reference — pushed via PushLvalue.
    /// upgrading can fail if the owning variable was freed.
    WeakLvalue(WeakValue),
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Integer(n) => *n != 0,
            Value::Float(f) => *f != 0.0,
            Value::Complex { re, im } => *re != 0.0 || *im != 0.0,
            _ => false,
        }
    }

    /// read through an lvalue or weak lvalue
    /// if not an lvalue, returns self.
    pub fn elide_lvalue(self) -> Value {
        match self {
            Value::Lvalue(rc) => rc.borrow().clone(),
            Value::WeakLvalue(weak) => {
                weak.upgrade()
                    .map(|rc| rc.borrow().clone())
                    .unwrap()
            }
            other => other
        }
    }

    pub fn force_elide_lvalue(&self) -> Value {
        match self {
            Value::Lvalue(rc) => rc.borrow().clone(),
            Value::WeakLvalue(weak) => {
                weak.upgrade()
                    .map(|rc| rc.borrow().clone())
                    .unwrap()
            }
            _ => panic!("Expected Lvalue")
        }
    }

    /// try to get the underlying RcValue (upgrading weak refs).
    /// returns None if this isn't an lvalue variant
    pub fn as_lvalue_rc(&self) -> Option<RcValue> {
        match self {
            Value::Lvalue(rc) => Some(rc.clone()),
            Value::WeakLvalue(weak) => Some(weak.upgrade().unwrap()),
            _ => None,
        }
    }

    pub fn is_lvalue(&self) -> bool {
        matches!(self, Value::Lvalue(_) | Value::WeakLvalue(_))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Float(_) => "float",
            Value::Integer(_) => "int",
            Value::Complex { .. } => "complex",
            Value::String(_) => "string",
            Value::PrimitiveMesh(_) => "mesh",
            Value::PrimitiveAnim(_) => "primitive_anim",
            Value::Lambda(_) => "lambda",
            Value::Operator(_) => "operator",
            Value::AnimBlock(_) => "anim_block",
            Value::Map(_) => "map",
            Value::List(_) => "list",
            Value::Stateful(_) => "stateful",
            Value::Leader(_) => "leader",
            Value::InvokedOperator(_) => "live operator",
            Value::InvokedFunction(_) => "live function",
            Value::Lvalue(_) => "lvalue",
            Value::WeakLvalue(_) => "lvalue",
        }
    }
}
