use std::{cell::Cell, future::Future, pin::Pin, rc::Rc};

use smallvec::SmallVec;

use crate::{
    error::ExecutorError,
    executor::invoke::fill_defaults,
    value::{
        container::{HashableKey, List, Map},
        invoked_function::InvokedFunction,
        invoked_operator::{extract_operator_result, InvokedOperator},
        rc_value, InstructionPointer, Value,
    },
};

use super::Executor;

impl Executor {
    /// interpolate between `a` and `b` at parameter `t` in [0, 1].
    ///
    /// rules (in priority order):
    /// 1. `a == b` (primitive equality) → `a`
    /// 2. both numbers → linear blend
    /// 3. both `InvokedFunction` of the same lambda (same IP) → lerp args element-wise
    /// 4. `InvokedOperator`s recurse by operator stack depth; equal-depth matching stacks lerp
    ///    operands and labeled args, otherwise the deeper side pops one operator layer
    pub fn lerp<'a>(
        &'a mut self,
        a: Value,
        b: Value,
        t: f64,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            let a = a.elide_lvalue();
            let b = b.elide_lvalue();
            if Value::values_equal(&a, &b) {
                return Ok(a);
            }

            let a_operator_count = operator_count(&a);
            let b_operator_count = operator_count(&b);

            if a_operator_count > b_operator_count {
                if let Value::InvokedOperator(ref a_inv) = a {
                    return self.lerp_popped_operator_lhs(a_inv, b.clone(), t).await;
                }
            }

            if b_operator_count > a_operator_count {
                if let Value::InvokedOperator(ref b_inv) = b {
                    return self.lerp_popped_operator_rhs(a.clone(), b_inv, t).await;
                }
            }

            if a_operator_count > 0 {
                if let (Value::InvokedOperator(a_inv), Value::InvokedOperator(b_inv)) = (&a, &b) {
                    return self.lerp_invoked_operators(a_inv, b_inv, t).await;
                }
            }

            if let (Value::InvokedFunction(a_inv), Value::InvokedFunction(b_inv)) = (&a, &b) {
                if let Some(ip) = same_lambda_ip(&a_inv.lambda, &b_inv.lambda) {
                    if a_inv.arguments.len() == b_inv.arguments.len() {
                        return self.lerp_invoked_functions(a_inv, b_inv, ip, t).await;
                    }
                }
            }

            let a = a.elide_wrappers(self).await?;
            let b = b.elide_wrappers(self).await?;

            if let Some(lerped) = self.lerp_containers(&a, &b, t).await? {
                return Ok(lerped);
            }

            lerp_numbers(a, b, t)
        })
    }

    fn lerp_invoked_functions<'a>(
        &'a mut self,
        a_inv: &'a Rc<InvokedFunction>,
        b_inv: &'a Rc<InvokedFunction>,
        _ip: InstructionPointer,
        t: f64,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            let a_args: SmallVec<[Value; 8]> = a_inv.arguments.clone();
            let b_args: SmallVec<[Value; 8]> = b_inv.arguments.clone();
            let lambda_val = a_inv.lambda.as_ref().clone();
            let labels = a_inv.labels.clone();

            if a_inv.labels != b_inv.labels {
                return Err(ExecutorError::Other(format!(
                    "cannot lerp invoked functions with different labeled arguments ({})",
                    format_label_mismatch(
                        &a_inv.labels,
                        &b_inv.labels,
                        a_args.len().max(b_args.len())
                    )
                )));
            }

            let mut lerped_args: SmallVec<[Value; 8]> = SmallVec::new();
            for (index, (ai, bi)) in a_args.into_iter().zip(b_args.into_iter()).enumerate() {
                if label_name_at(&labels, index).is_some() {
                    lerped_args.push(self.lerp(ai, bi, t).await.map_err(|err| {
                        lerp_context(
                            format!("cannot lerp labeled function argument at index {}", index),
                            err,
                        )
                    })?);
                } else if Value::values_equal(&ai, &bi) {
                    lerped_args.push(ai);
                } else {
                    return Err(ExecutorError::Other(format!(
                        "cannot lerp invoked functions when unlabeled argument at index {} differs",
                        index
                    )));
                }
            }

            let lambda = match lambda_val.clone().elide_lvalue() {
                Value::Lambda(rc) => rc,
                other => return Err(ExecutorError::type_error("lambda", other.type_name())),
            };
            let full_args = fill_defaults(lerped_args.iter().cloned().collect(), &lambda);
            let result = self.eagerly_invoke_lambda(&lambda, &full_args).await?;

            let inv = InvokedFunction {
                lambda: Box::new(lambda_val),
                arguments: lerped_args,
                labels,
                cached_result: Cell::new(Some(Box::new(result))),
            };
            Ok(Value::InvokedFunction(Rc::new(inv)))
        })
    }

    fn lerp_invoked_operators<'a>(
        &'a mut self,
        a_inv: &'a Rc<InvokedOperator>,
        b_inv: &'a Rc<InvokedOperator>,
        t: f64,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            if !Value::values_equal(&a_inv.operator, &b_inv.operator) || a_inv.labels != b_inv.labels {
                return Err(ExecutorError::Other(format!(
                    "cannot lerp invoked operators with different operators or labeled arguments ({})",
                    format_label_mismatch(
                        &a_inv.labels,
                        &b_inv.labels,
                        a_inv.arguments.len().max(b_inv.arguments.len())
                    )
                )));
            }

            if a_inv.arguments.len() != b_inv.arguments.len() {
                return Err(ExecutorError::Other(format!(
                    "cannot lerp invoked operators with different arity: {} vs {}",
                    a_inv.arguments.len(),
                    b_inv.arguments.len()
                )));
            }

            let lerped_operand = self
                .lerp(a_inv.operand.as_ref().clone(), b_inv.operand.as_ref().clone(), t)
                .await
                .map_err(|err| lerp_context("cannot lerp operator operands".into(), err))?;

            let mut lerped_args = SmallVec::new();
            for (index, (ai, bi)) in a_inv
                .arguments
                .iter()
                .cloned()
                .zip(b_inv.arguments.iter().cloned())
                .enumerate()
            {
                if label_name_at(&a_inv.labels, index).is_some() {
                    lerped_args.push(self.lerp(ai, bi, t).await.map_err(|err| {
                        lerp_context(
                            format!("cannot lerp labeled operator argument at index {}", index),
                            err,
                        )
                    })?);
                } else if Value::values_equal(&ai, &bi) {
                    lerped_args.push(ai);
                } else {
                    return Err(ExecutorError::Other(format!(
                        "cannot lerp invoked operators when unlabeled argument at index {} differs",
                        index
                    )));
                }
            }

            let operator_value = a_inv.operator.as_ref().clone();
            let operator = match operator_value.clone().elide_lvalue() {
                Value::Operator(op) => op,
                other => return Err(ExecutorError::type_error("operator", other.type_name())),
            };

            let mut full_args: Vec<Value> = vec![lerped_operand.clone()];
            full_args.extend(lerped_args.iter().cloned());
            let full_args = fill_defaults(full_args, &operator.0);

            let raw = self.eagerly_invoke_lambda(&operator.0, &full_args).await?;
            let (initial, modified) = extract_operator_result(raw)?;

            Ok(Value::InvokedOperator(Rc::new(crate::value::invoked_operator::build_invoked_operator(
                operator_value,
                lerped_operand,
                lerped_args,
                a_inv.labels.clone(),
                initial,
                modified,
            ))))
        })
    }

    fn lerp_popped_operator_lhs<'a>(
        &'a mut self,
        inv: &'a Rc<InvokedOperator>,
        other: Value,
        t: f64,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            let mid = self.lerp(inv.operand.as_ref().clone(), other, t).await?;
            self.lerp_operator_embeds(inv, mid, t).await
        })
    }

    fn lerp_popped_operator_rhs<'a>(
        &'a mut self,
        other: Value,
        inv: &'a Rc<InvokedOperator>,
        t: f64,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            let mid = self.lerp(other, inv.operand.as_ref().clone(), t).await?;
            self.lerp_operator_embeds(inv, mid, t).await
        })
    }

    fn lerp_operator_embeds<'a>(
        &'a mut self,
        inv: &'a Rc<InvokedOperator>,
        mid: Value,
        t: f64,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            let operator = match inv.operator.as_ref().clone().elide_lvalue() {
                Value::Operator(op) => op,
                other => return Err(ExecutorError::type_error("operator", other.type_name())),
            };
            let mut full_args: Vec<Value> = vec![mid];
            full_args.extend(inv.arguments.iter().map(|b| b.clone().elide_lvalue()));
            let full_args = fill_defaults(full_args, &operator.0);

            let raw = self.eagerly_invoke_lambda(&operator.0, &full_args).await?;
            let (embed0, embed1) = extract_operator_result(raw)?;
            self.lerp(embed0, embed1, t).await
        })
    }

    fn lerp_containers<'a>(
        &'a mut self,
        a: &'a Value,
        b: &'a Value,
        t: f64,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Value>, ExecutorError>> + 'a>> {
        Box::pin(async move {
            match (a, b) {
                (Value::List(a_list), Value::List(b_list)) => {
                    if a_list.len() != b_list.len() {
                        return Err(ExecutorError::Other(format!(
                            "cannot lerp vectors of different lengths: {} vs {}",
                            a_list.len(),
                            b_list.len()
                        )));
                    }

                    let mut elements = Vec::with_capacity(a_list.len());
                    for (index, (a_elem, b_elem)) in
                        a_list.elements.iter().zip(&b_list.elements).enumerate()
                    {
                        elements.push(rc_value(
                            self.lerp(a_elem.borrow().clone(), b_elem.borrow().clone(), t)
                                .await
                                .map_err(|err| {
                                    lerp_context(
                                        format!("cannot lerp vector element at index {}", index),
                                        err,
                                    )
                                })?,
                        ));
                    }
                    Ok(Some(Value::List(Rc::new(List { elements }))))
                }
                (Value::Map(a_map), Value::Map(b_map)) => {
                    let missing_from_b: Vec<&HashableKey> = a_map
                        .insertion_order
                        .iter()
                        .filter(|key| !b_map.contains_key(key))
                        .collect();
                    let missing_from_a: Vec<&HashableKey> = b_map
                        .insertion_order
                        .iter()
                        .filter(|key| !a_map.contains_key(key))
                        .collect();
                    if !missing_from_a.is_empty() || !missing_from_b.is_empty() {
                        return Err(ExecutorError::Other(format!(
                            "cannot lerp maps with different keys ({})",
                            format_missing_keys(&missing_from_a, &missing_from_b)
                        )));
                    }

                    let mut map = Map::new();
                    for key in &a_map.insertion_order {
                        let a_value = a_map.get(key).expect("map key missing from entries");
                        let b_value = b_map.get(key).expect("map key missing from entries");
                        map.insert(
                            key.clone(),
                            rc_value(
                                self.lerp(a_value.borrow().clone(), b_value.borrow().clone(), t)
                                    .await
                                    .map_err(|err| {
                                        lerp_context(
                                            format!(
                                                "cannot lerp map value at key {}",
                                                format_hashable_key(key)
                                            ),
                                            err,
                                        )
                                    })?,
                            ),
                        );
                    }
                    Ok(Some(Value::Map(Rc::new(map))))
                }
                _ => Ok(None),
            }
        })
    }
}

fn same_lambda_ip(a: &Value, b: &Value) -> Option<InstructionPointer> {
    let a_ip = match a.clone().elide_lvalue() {
        Value::Lambda(rc) => rc.ip,
        _ => return None,
    };
    let b_ip = match b.clone().elide_lvalue() {
        Value::Lambda(rc) => rc.ip,
        _ => return None,
    };
    (a_ip == b_ip).then_some(a_ip)
}

fn operator_count(value: &Value) -> usize {
    match value {
        Value::Lvalue(rc) => {
            let value = rc.borrow();
            operator_count(&value)
        }
        Value::WeakLvalue(weak) => {
            let rc = weak.upgrade().unwrap();
            let value = rc.borrow();
            operator_count(&value)
        }
        Value::InvokedOperator(inv) => 1 + operator_count(inv.operand.as_ref()),
        _ => 0,
    }
}

fn lerp_numbers(a: Value, b: Value, t: f64) -> Result<Value, ExecutorError> {
    let s = 1.0 - t;
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => Ok(Value::Float(s * x + t * y)),
        (Value::Integer(x), Value::Integer(y)) => Ok(Value::Float(s * x as f64 + t * y as f64)),
        (Value::Float(x), Value::Integer(y)) => Ok(Value::Float(s * x + t * y as f64)),
        (Value::Integer(x), Value::Float(y)) => Ok(Value::Float(s * x as f64 + t * y)),
        (Value::Complex { re: ar, im: ai }, Value::Complex { re: br, im: bi }) => {
            Ok(Value::Complex {
                re: s * ar + t * br,
                im: s * ai + t * bi,
            })
        }
        (a, b) => Err(ExecutorError::Other(format!(
            "cannot lerp {} and {}",
            a.type_name(),
            b.type_name()
        ))),
    }
}

fn lerp_context(context: String, err: ExecutorError) -> ExecutorError {
    ExecutorError::Other(format!("{}: {}", context, err))
}

fn format_missing_keys(missing_from_a: &[&HashableKey], missing_from_b: &[&HashableKey]) -> String {
    let mut parts = Vec::new();
    if !missing_from_a.is_empty() {
        parts.push(format!(
            "missing from first map [{}]",
            missing_from_a
                .iter()
                .map(|key| format_hashable_key(key))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !missing_from_b.is_empty() {
        parts.push(format!(
            "missing from second map [{}]",
            missing_from_b
                .iter()
                .map(|key| format_hashable_key(key))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    parts.join("; ")
}

fn format_hashable_key(key: &HashableKey) -> String {
    match key {
        HashableKey::Integer(n) => n.to_string(),
        HashableKey::String(s) => format!("{:?}", s),
        HashableKey::Vector(keys) => format!(
            "[{}]",
            keys.iter()
                .map(format_hashable_key)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn label_name_at<'a>(labels: &'a SmallVec<[(usize, String); 4]>, index: usize) -> Option<&'a str> {
    labels
        .iter()
        .find_map(|(label_index, name)| (*label_index == index).then_some(name.as_str()))
}

fn format_label_mismatch(
    a_labels: &SmallVec<[(usize, String); 4]>,
    b_labels: &SmallVec<[(usize, String); 4]>,
    len: usize,
) -> String {
    let mut mismatches = Vec::new();
    for index in 0..len {
        let a_label = label_name_at(a_labels, index);
        let b_label = label_name_at(b_labels, index);
        if a_label != b_label {
            mismatches.push(format!(
                "index {}: {} vs {}",
                index,
                format_optional_label(a_label),
                format_optional_label(b_label)
            ));
        }
    }

    if mismatches.is_empty() {
        "label sets differ".into()
    } else {
        mismatches.join(", ")
    }
}

fn format_optional_label(label: Option<&str>) -> String {
    match label {
        Some(label) => format!("{:?}", label),
        None => "unlabeled".into(),
    }
}
