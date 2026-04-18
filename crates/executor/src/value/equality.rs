use std::rc::Rc;
use std::sync::Arc;

use super::{
    Value,
    primitive_anim::PrimitiveAnim,
    stateful::StatefulNode,
};

impl Value {
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
                    .is_some_and(|rc| Value::values_equal(&rc.borrow(), b));
            }
            _ => {}
        }
        match b {
            Value::Lvalue(rc) => return Value::values_equal(a, &rc.borrow()),
            Value::WeakLvalue(weak) => {
                return weak
                    .upgrade()
                    .is_some_and(|rc| Value::values_equal(a, &rc.borrow()));
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
            (Value::Mesh(a), Value::Mesh(b)) => Arc::ptr_eq(a, b),

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
                            b.get(k).is_some_and(|bv| {
                                Rc::ptr_eq(av, bv)
                                    || Value::values_equal(&av.borrow(), &bv.borrow())
                            })
                        }))
            }

            (Value::Stateful(a), Value::Stateful(b)) => stateful_equal(&a.root, &b.root),

            // leaders compared by identity of the leader cell
            (Value::Leader(a), Value::Leader(b)) => {
                Value::values_equal(&a.leader_rc.borrow(), &b.leader_rc.borrow())
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
