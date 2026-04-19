use std::rc::Rc;
use std::sync::Arc;

use crate::heap::with_heap;

use super::{Value, primitive_anim::PrimitiveAnim, stateful::StatefulNode};

impl Value {
    /// structural equality for all value types.
    pub fn values_equal(a: &Value, b: &Value) -> bool {
        match a {
            Value::Lvalue(vrc) => {
                return Value::values_equal(&with_heap(|h| h.get(vrc.key()).clone()), b);
            }
            Value::WeakLvalue(vweak) => {
                return Value::values_equal(&with_heap(|h| h.get(vweak.key()).clone()), b);
            }
            _ => {}
        }
        match b {
            Value::Lvalue(vrc) => {
                return Value::values_equal(a, &with_heap(|h| h.get(vrc.key()).clone()));
            }
            Value::WeakLvalue(vweak) => {
                return Value::values_equal(a, &with_heap(|h| h.get(vweak.key()).clone()));
            }
            _ => {}
        }

        match (a, b) {
            (Value::Nil, Value::Nil) => true,
            (Value::Integer(x), Value::Integer(y)) => x == y,
            (Value::Float(x), Value::Float(y)) => x == y,
            (Value::Integer(x), Value::Float(y)) => (*x as f64) == *y,
            (Value::Float(x), Value::Integer(y)) => *x == (*y as f64),
            (Value::Complex { re: ar, im: ai }, Value::Complex { re: br, im: bi }) => {
                ar == br && ai == bi
            }
            (Value::String(x), Value::String(y)) => x == y,

            (Value::Lambda(a), Value::Lambda(b)) => Rc::ptr_eq(a, b) || a.ip == b.ip,
            (Value::Operator(a), Value::Operator(b)) => Rc::ptr_eq(&a.0, &b.0) || a.0.ip == b.0.ip,
            (Value::AnimBlock(a), Value::AnimBlock(b)) => Rc::ptr_eq(a, b),
            (Value::Mesh(a), Value::Mesh(b)) => Arc::ptr_eq(a, b),

            (Value::PrimitiveAnim(a), Value::PrimitiveAnim(b)) => prim_anim_equal(a, b),

            (Value::List(a), Value::List(b)) => {
                Rc::ptr_eq(a, b)
                    || (a.len() == b.len()
                        && a.elements.iter().zip(b.elements.iter()).all(|(ak, bk)| {
                            ak == bk
                                || Value::values_equal(
                                    &with_heap(|h| h.get(ak.key()).clone()),
                                    &with_heap(|h| h.get(bk.key()).clone()),
                                )
                        }))
            }
            (Value::Map(a), Value::Map(b)) => {
                Rc::ptr_eq(a, b)
                    || (a.len() == b.len()
                        && a.iter().all(|(k, av)| {
                            b.get(k).is_some_and(|bv| {
                                av == bv
                                    || Value::values_equal(
                                        &with_heap(|h| h.get(av.key()).clone()),
                                        &with_heap(|h| h.get(bv.key()).clone()),
                                    )
                            })
                        }))
            }

            (Value::Stateful(a), Value::Stateful(b)) => {
                a.cache.read_kind == b.cache.read_kind && stateful_equal(&a.body.root, &b.body.root)
            }

            (Value::Leader(a), Value::Leader(b)) => {
                Value::values_equal(
                    &with_heap(|h| h.get(a.leader_rc.key()).clone()),
                    &with_heap(|h| h.get(b.leader_rc.key()).clone()),
                )
            }

            (Value::InvokedFunction(a), Value::InvokedFunction(b)) => {
                Rc::ptr_eq(&a.body, &b.body)
                    || (Value::values_equal(&a.body.lambda, &b.body.lambda)
                        && a.body.labels == b.body.labels
                        && a.body.arguments.len() == b.body.arguments.len()
                        && a.body
                            .arguments
                            .iter()
                            .zip(b.body.arguments.iter())
                            .all(|(ai, bi)| Value::values_equal(ai, bi)))
            }
            (Value::InvokedOperator(a), Value::InvokedOperator(b)) => {
                Rc::ptr_eq(&a.body, &b.body)
                    || (Value::values_equal(&a.body.operator, &b.body.operator)
                        && Value::values_equal(&a.body.operand, &b.body.operand)
                        && a.body.labels == b.body.labels
                        && a.body.arguments.len() == b.body.arguments.len()
                        && a.body
                            .arguments
                            .iter()
                            .zip(b.body.arguments.iter())
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
        (StatefulNode::LeaderRef(a), StatefulNode::LeaderRef(b)) => a == b,
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
                && aa.iter().zip(ba.iter()).all(|(ak, bk)| {
                    ak == bk
                        || Value::values_equal(
                            &with_heap(|h| h.get(ak.key()).clone()),
                            &with_heap(|h| h.get(bk.key()).clone()),
                        )
                })
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
                && Value::values_equal(
                    &with_heap(|h| h.get(aop.key()).clone()),
                    &with_heap(|h| h.get(bop.key()).clone()),
                )
                && aa.len() == ba.len()
                && aa.iter().zip(ba.iter()).all(|(ak, bk)| {
                    ak == bk
                        || Value::values_equal(
                            &with_heap(|h| h.get(ak.key()).clone()),
                            &with_heap(|h| h.get(bk.key()).clone()),
                        )
                })
        }
        _ => false,
    }
}
