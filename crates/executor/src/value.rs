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
use std::sync::Arc;

use crate::error::ExecutorError;
use crate::executor::Executor;

use self::{
    anim_block::AnimBlock,
    container::{List, Map},
    invoked_function::InvokedFunction,
    invoked_operator::InvokedOperator,
    lambda::{Lambda, Operator},
    leader::Leader,
    primitive_anim::PrimitiveAnim,
    primitive_mesh::PrimitiveMesh,
    stateful::{Stateful, StatefulNode},
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

    PrimitiveMesh(Arc<PrimitiveMesh>),
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

impl Value {
    pub fn check_truthy(&self) -> Result<bool, ExecutorError> {
        match self {
            Value::Integer(n) => Ok(*n != 0),
            Value::Float(f) => Ok(*f != 0.0),
            Value::Complex { re, im } => Ok(*re != 0.0 || *im != 0.0),
            _ => Err(ExecutorError::InvalidCondition(self.type_name())),
        }
    }

    // an element might contain lvalues if it is itself an lvalue or a nested list
    fn may_need_lvalue_elision(&self) -> bool {
        self.is_lvalue() || matches!(self, Value::List(_))
    }

    // creates owned copy of self which elides all lvalues, recursing on lists and maps
    pub fn elide_lvalue_rec(self) -> Value {
        match self {
            Value::Lvalue(rc) => rc.borrow().clone().elide_lvalue_rec(),
            Value::WeakLvalue(weak) => weak
                .upgrade()
                .map(|rc| rc.borrow().clone().elide_lvalue_rec())
                .unwrap(),
            Value::List(mut list) => {
                if !list
                    .elements
                    .iter()
                    .any(|e| e.borrow().may_need_lvalue_elision())
                {
                    return Value::List(list);
                }

                let list_mut = Rc::make_mut(&mut list);
                for elem in &mut list_mut.elements {
                    if !elem.borrow().may_need_lvalue_elision() {
                        continue;
                    }
                    let elided = elem.borrow().clone().elide_lvalue_rec();
                    // reuse the existing allocation when exclusively owned; COW otherwise
                    if Rc::strong_count(elem) == 1 {
                        *elem.borrow_mut() = elided;
                    } else {
                        *elem = rc_value(elided);
                    }
                }
                Value::List(list)
            }
            other => other,
        }
    }

    /// read through an lvalue or weak lvalue
    /// if not an lvalue, returns self.
    pub fn elide_lvalue(self) -> Value {
        match self {
            Value::Lvalue(rc) => rc.borrow().clone(),
            Value::WeakLvalue(weak) => weak.upgrade().map(|rc| rc.borrow().clone()).unwrap(),
            other => other,
        }
    }

    pub async fn elide_wrappers(self, executor: &mut Executor) -> Result<Value, ExecutorError> {
        let mut base = self.elide_lvalue();
        loop {
            base = match base {
                Value::Leader(leader) => leader.leader_rc.borrow().clone(),
                Value::InvokedOperator(op) => InvokedOperator::value(&op, executor).await?,
                Value::InvokedFunction(func) => InvokedFunction::value(&func, executor).await?,
                other => return Ok(other),
            };
        }
    }

    pub fn force_elide_lvalue(&self) -> Value {
        match self {
            Value::Lvalue(rc) => rc.borrow().clone(),
            Value::WeakLvalue(weak) => weak.upgrade().map(|rc| rc.borrow().clone()).unwrap(),
            _ => panic!("Expected Lvalue"),
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

    /// structural equality for all value types.
    /// lvalues are transparently dereferenced.
    /// for InvokedFunction/InvokedOperator: compares by lambda identity + arguments + labels,
    /// not by the computed result.
    /// Rc/pointer equality is used as a fast path where applicable.
    pub fn values_equal(a: &Value, b: &Value) -> bool {
        // elide lvalue wrappers without cloning where possible
        match a {
            Value::Lvalue(rc) => return Value::values_equal(&rc.borrow(), b),
            Value::WeakLvalue(weak) => {
                return weak
                    .upgrade()
                    .map_or(false, |rc| Value::values_equal(&rc.borrow(), b));
            }
            _ => {}
        }
        match b {
            Value::Lvalue(rc) => return Value::values_equal(a, &rc.borrow()),
            Value::WeakLvalue(weak) => {
                return weak
                    .upgrade()
                    .map_or(false, |rc| Value::values_equal(a, &rc.borrow()));
            }
            _ => {}
        }

        match (a, b) {
            (Value::Nil, Value::Nil) => true,
            (Value::Integer(x), Value::Integer(y)) => x == y,
            (Value::Float(x), Value::Float(y)) => x == y,
            // cross-type numeric equality
            (Value::Integer(x), Value::Float(y)) => (*x as f64) == *y,
            (Value::Float(x), Value::Integer(y)) => *x == (*y as f64),
            (Value::Complex { re: ar, im: ai }, Value::Complex { re: br, im: bi }) => {
                ar == br && ai == bi
            }
            (Value::String(x), Value::String(y)) => x == y,

            (Value::Lambda(a), Value::Lambda(b)) => Rc::ptr_eq(a, b) || a.ip == b.ip,
            (Value::Operator(a), Value::Operator(b)) => Rc::ptr_eq(&a.0, &b.0) || a.0.ip == b.0.ip,
            // anim blocks are identity-equal (playing one consumes it)
            (Value::AnimBlock(a), Value::AnimBlock(b)) => Rc::ptr_eq(a, b),
            // meshes compared by pointer (deep mesh equality would be expensive)
            (Value::PrimitiveMesh(a), Value::PrimitiveMesh(b)) => Arc::ptr_eq(a, b),

            (Value::PrimitiveAnim(a), Value::PrimitiveAnim(b)) => prim_anim_equal(a, b),

            (Value::List(a), Value::List(b)) => {
                Rc::ptr_eq(a, b)
                    || (a.len() == b.len()
                        && a.elements.iter().zip(b.elements.iter()).all(|(ae, be)| {
                            Rc::ptr_eq(ae, be) || Value::values_equal(&ae.borrow(), &be.borrow())
                        }))
            }
            (Value::Map(a), Value::Map(b)) => {
                Rc::ptr_eq(a, b)
                    || (a.len() == b.len()
                        && a.iter().all(|(k, av)| {
                            b.get(k).map_or(false, |bv| {
                                Rc::ptr_eq(av, bv)
                                    || Value::values_equal(&av.borrow(), &bv.borrow())
                            })
                        }))
            }

            (Value::Stateful(a), Value::Stateful(b)) => stateful_equal(&a.root, &b.root),

            // leaders compared by identity of the leader cell
            (Value::Leader(a), Value::Leader(b)) => {
                Value::values_equal(&*a.leader_rc.borrow(), &*b.leader_rc.borrow())
            }

            // InvokedFunction: same lambda + same args + same labels
            (Value::InvokedFunction(a), Value::InvokedFunction(b)) => {
                Rc::ptr_eq(a, b)
                    || (Value::values_equal(&a.lambda, &b.lambda)
                        && a.labels == b.labels
                        && a.arguments.len() == b.arguments.len()
                        && a.arguments
                            .iter()
                            .zip(b.arguments.iter())
                            .all(|(ai, bi)| Value::values_equal(ai, bi)))
            }
            // InvokedOperator: same operator + same operand + same args + same labels
            (Value::InvokedOperator(a), Value::InvokedOperator(b)) => {
                Rc::ptr_eq(a, b)
                    || (Value::values_equal(&a.operator, &b.operator)
                        && Value::values_equal(&a.operand, &b.operand)
                        && a.labels == b.labels
                        && a.arguments.len() == b.arguments.len()
                        && a.arguments
                            .iter()
                            .zip(b.arguments.iter())
                            .all(|(ai, bi)| Value::values_equal(ai, bi)))
            }

            _ => false,
        }
    }
}

fn prim_anim_equal(a: &PrimitiveAnim, b: &PrimitiveAnim) -> bool {
    match (a, b) {
        (PrimitiveAnim::Set { candidates: a }, PrimitiveAnim::Set { candidates: b }) => {
            Value::values_equal(a, b)
        }
        (PrimitiveAnim::Wait { time: ta }, PrimitiveAnim::Wait { time: tb }) => ta == tb,
        (
            PrimitiveAnim::Lerp {
                candidates: ca,
                time: ta,
                progression: pa,
            },
            PrimitiveAnim::Lerp {
                candidates: cb,
                time: tb,
                progression: pb,
            },
        ) => {
            ta == tb
                && Value::values_equal(ca, cb)
                && match (pa, pb) {
                    (None, None) => true,
                    (Some(a), Some(b)) => Value::values_equal(a, b),
                    _ => false,
                }
        }
        _ => false,
    }
}

fn stateful_equal(a: &StatefulNode, b: &StatefulNode) -> bool {
    match (a, b) {
        (StatefulNode::LeaderRef(a), StatefulNode::LeaderRef(b)) => Rc::ptr_eq(a, b),
        (StatefulNode::Constant(a), StatefulNode::Constant(b)) => Value::values_equal(a, b),
        (
            StatefulNode::LabeledCall {
                func: af,
                args: aa,
                labels: al,
            },
            StatefulNode::LabeledCall {
                func: bf,
                args: ba,
                labels: bl,
            },
        ) => {
            al == bl
                && stateful_equal(af, bf)
                && aa.len() == ba.len()
                && aa.iter().zip(ba.iter()).all(|(a, b)| stateful_equal(a, b))
        }
        (
            StatefulNode::LabeledOperatorCall {
                operator: ao,
                operand: aop,
                extra_args: aa,
                labels: al,
            },
            StatefulNode::LabeledOperatorCall {
                operator: bo,
                operand: bop,
                extra_args: ba,
                labels: bl,
            },
        ) => {
            al == bl
                && stateful_equal(ao, bo)
                && stateful_equal(aop, bop)
                && aa.len() == ba.len()
                && aa.iter().zip(ba.iter()).all(|(a, b)| stateful_equal(a, b))
        }
        _ => false,
    }
}
