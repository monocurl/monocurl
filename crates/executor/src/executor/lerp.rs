use std::{future::Future, pin::Pin, rc::Rc, sync::Arc};

use geo::{
    mesh::{Dot, Lin, LinVertex, Mesh, Tri, TriVertex, Uniforms},
    simd::{Float2, Float3, Float4},
};
use smallvec::SmallVec;

use crate::{
    error::ExecutorError,
    executor::fill_defaults,
    heap::with_heap,
    value::{
        InstructionPointer, Value,
        container::{HashableKey, List, Map},
        invoked_function::{InvokedFunction, make_invoked_function},
        invoked_operator::{InvokedOperator, extract_operator_result, make_invoked_operator},
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
                if let Some(ip) = same_lambda_ip(&a_inv.body.lambda, &b_inv.body.lambda) {
                    if a_inv.body.arguments.len() == b_inv.body.arguments.len() {
                        return self.lerp_invoked_functions(a_inv, b_inv, ip, t).await;
                    }
                }
            }

            let a = a.elide_wrappers(self).await?;
            let b = b.elide_wrappers(self).await?;

            if let Some(lerped) = lerp_meshes(&a, &b, t)? {
                return Ok(lerped);
            }

            if let Some(lerped) = self.lerp_containers(&a, &b, t).await? {
                return Ok(lerped);
            }

            lerp_numbers(a, b, t)
        })
    }

    fn lerp_invoked_functions<'a>(
        &'a mut self,
        a_inv: &'a InvokedFunction,
        b_inv: &'a InvokedFunction,
        _ip: InstructionPointer,
        t: f64,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            let a_args: SmallVec<[Value; 8]> = a_inv.body.arguments.clone();
            let b_args: SmallVec<[Value; 8]> = b_inv.body.arguments.clone();
            let lambda_val = a_inv.body.lambda.as_ref().clone();
            let labels = a_inv.body.labels.clone();

            if a_inv.body.labels != b_inv.body.labels {
                return Err(ExecutorError::Other(format!(
                    "cannot lerp invoked functions with different labeled arguments ({})",
                    format_label_mismatch(
                        &a_inv.body.labels,
                        &b_inv.body.labels,
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
            let result = self
                .eagerly_invoke_lambda(&lambda, &full_args, None)
                .await?;

            Ok(Value::InvokedFunction(make_invoked_function(
                lambda_val,
                lerped_args,
                labels,
                Some(result),
            )))
        })
    }

    fn lerp_invoked_operators<'a>(
        &'a mut self,
        a_inv: &'a InvokedOperator,
        b_inv: &'a InvokedOperator,
        t: f64,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            if !Value::values_equal(&a_inv.body.operator, &b_inv.body.operator)
                || a_inv.body.labels != b_inv.body.labels
            {
                return Err(ExecutorError::Other(format!(
                    "cannot lerp invoked operators with different operators or labeled arguments ({})",
                    format_label_mismatch(
                        &a_inv.body.labels,
                        &b_inv.body.labels,
                        a_inv.body.arguments.len().max(b_inv.body.arguments.len())
                    )
                )));
            }

            if a_inv.body.arguments.len() != b_inv.body.arguments.len() {
                return Err(ExecutorError::Other(format!(
                    "cannot lerp invoked operators with different arity: {} vs {}",
                    a_inv.body.arguments.len(),
                    b_inv.body.arguments.len()
                )));
            }

            let lerped_operand = self
                .lerp(
                    a_inv.body.operand.as_ref().clone(),
                    b_inv.body.operand.as_ref().clone(),
                    t,
                )
                .await
                .map_err(|err| lerp_context("cannot lerp operator operands".into(), err))?;

            let mut lerped_args = SmallVec::new();
            for (index, (ai, bi)) in a_inv
                .body
                .arguments
                .iter()
                .cloned()
                .zip(b_inv.body.arguments.iter().cloned())
                .enumerate()
            {
                if label_name_at(&a_inv.body.labels, index).is_some() {
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

            let operator_value = a_inv.body.operator.as_ref().clone();
            let operator = match operator_value.clone().elide_lvalue() {
                Value::Operator(op) => op,
                other => return Err(ExecutorError::type_error("operator", other.type_name())),
            };

            let mut full_args: Vec<Value> = vec![lerped_operand.clone()];
            full_args.extend(lerped_args.iter().cloned());
            let full_args = fill_defaults(full_args, &operator.0);

            let raw = self
                .eagerly_invoke_lambda(&operator.0, &full_args, None)
                .await?;
            let (initial, modified) = extract_operator_result(raw)?;

            Ok(Value::InvokedOperator(make_invoked_operator(
                operator_value,
                lerped_operand,
                lerped_args,
                a_inv.body.labels.clone(),
                initial,
                modified,
            )))
        })
    }

    fn lerp_popped_operator_lhs<'a>(
        &'a mut self,
        inv: &'a InvokedOperator,
        other: Value,
        t: f64,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            let mid = self
                .lerp(inv.body.operand.as_ref().clone(), other, t)
                .await?;
            self.lerp_operator_embeds(inv, mid, t).await
        })
    }

    fn lerp_popped_operator_rhs<'a>(
        &'a mut self,
        other: Value,
        inv: &'a InvokedOperator,
        t: f64,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            let mid = self
                .lerp(other, inv.body.operand.as_ref().clone(), t)
                .await?;
            self.lerp_operator_embeds(inv, mid, t).await
        })
    }

    fn lerp_operator_embeds<'a>(
        &'a mut self,
        inv: &'a InvokedOperator,
        mid: Value,
        t: f64,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            let operator = match inv.body.operator.as_ref().clone().elide_lvalue() {
                Value::Operator(op) => op,
                other => return Err(ExecutorError::type_error("operator", other.type_name())),
            };
            let mut full_args: Vec<Value> = vec![mid];
            full_args.extend(inv.body.arguments.iter().map(|b| b.clone().elide_lvalue()));
            let full_args = fill_defaults(full_args, &operator.0);

            let raw = self
                .eagerly_invoke_lambda(&operator.0, &full_args, None)
                .await?;
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

                    let mut elements = SmallVec::with_capacity(a_list.len());
                    for (index, (a_key, b_key)) in
                        a_list.elements.iter().zip(&b_list.elements).enumerate()
                    {
                        let a_val = with_heap(|h| h.get(a_key.key()).clone());
                        let b_val = with_heap(|h| h.get(b_key.key()).clone());
                        let lerped = self.lerp(a_val, b_val, t).await.map_err(|err| {
                            lerp_context(
                                format!("cannot lerp vector element at index {}", index),
                                err,
                            )
                        })?;
                        elements.push(crate::heap::VRc::new(lerped));
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
                        let a_key = a_map.get(key).unwrap();
                        let b_key = b_map.get(key).unwrap();
                        let a_val = with_heap(|h| h.get(a_key.key()).clone());
                        let b_val = with_heap(|h| h.get(b_key.key()).clone());
                        let lerped = self.lerp(a_val, b_val, t).await.map_err(|err| {
                            lerp_context(
                                format!(
                                    "cannot lerp map value at key {}",
                                    format_hashable_key(key)
                                ),
                                err,
                            )
                        })?;
                        map.insert(key.clone(), crate::heap::VRc::new(lerped));
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
        Value::Lvalue(vrc) => {
            let inner = with_heap(|h| h.get(vrc.key()).clone());
            operator_count(&inner)
        }
        Value::WeakLvalue(vweak) => {
            let inner = with_heap(|h| h.get(vweak.key()).clone());
            operator_count(&inner)
        }
        Value::InvokedOperator(inv) => 1 + operator_count(inv.body.operand.as_ref()),
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

fn lerp_meshes(a: &Value, b: &Value, t: f64) -> Result<Option<Value>, ExecutorError> {
    let (Value::Mesh(a), Value::Mesh(b)) = (a, b) else {
        return Ok(None);
    };

    if a.dots.len() != b.dots.len() || a.lins.len() != b.lins.len() || a.tris.len() != b.tris.len()
    {
        return Err(ExecutorError::Other(format!(
            "cannot lerp meshes with different topology counts: dots {} vs {}, lins {} vs {}, tris {} vs {}",
            a.dots.len(),
            b.dots.len(),
            a.lins.len(),
            b.lins.len(),
            a.tris.len(),
            b.tris.len()
        )));
    }

    let mesh = Mesh {
        dots: a
            .dots
            .iter()
            .zip(&b.dots)
            .map(|(a, b)| Dot {
                pos: lerp_float3(a.pos, b.pos, t),
                norm: lerp_float3(a.norm, b.norm, t),
                col: lerp_float4(a.col, b.col, t),
                inv: b.inv,
                anti: b.anti,
                is_dom_sib: b.is_dom_sib,
            })
            .collect(),
        lins: a
            .lins
            .iter()
            .zip(&b.lins)
            .map(|(a, b)| Lin {
                a: LinVertex {
                    pos: lerp_float3(a.a.pos, b.a.pos, t),
                    col: lerp_float4(a.a.col, b.a.col, t),
                },
                b: LinVertex {
                    pos: lerp_float3(a.b.pos, b.b.pos, t),
                    col: lerp_float4(a.b.col, b.b.col, t),
                },
                norm: lerp_float3(a.norm, b.norm, t),
                prev: b.prev,
                next: b.next,
                inv: b.inv,
                anti: b.anti,
                is_dom_sib: b.is_dom_sib,
            })
            .collect(),
        tris: a
            .tris
            .iter()
            .zip(&b.tris)
            .map(|(a, b)| Tri {
                a: TriVertex {
                    pos: lerp_float3(a.a.pos, b.a.pos, t),
                    col: lerp_float4(a.a.col, b.a.col, t),
                    uv: lerp_float2(a.a.uv, b.a.uv, t),
                },
                b: TriVertex {
                    pos: lerp_float3(a.b.pos, b.b.pos, t),
                    col: lerp_float4(a.b.col, b.b.col, t),
                    uv: lerp_float2(a.b.uv, b.b.uv, t),
                },
                c: TriVertex {
                    pos: lerp_float3(a.c.pos, b.c.pos, t),
                    col: lerp_float4(a.c.col, b.c.col, t),
                    uv: lerp_float2(a.c.uv, b.c.uv, t),
                },
                ab: b.ab,
                bc: b.bc,
                ca: b.ca,
                anti: b.anti,
                is_dom_sib: b.is_dom_sib,
            })
            .collect(),
        uniform: lerp_uniforms(&a.uniform, &b.uniform, t),
        tag: if t < 0.5 { a.tag.clone() } else { b.tag.clone() },
    };

    Ok(Some(Value::Mesh(Arc::new(mesh))))
}

fn lerp_uniforms(a: &Uniforms, b: &Uniforms, t: f64) -> Uniforms {
    Uniforms {
        alpha: (1.0 - t) * a.alpha + t * b.alpha,
        img: if t < 0.5 { a.img.clone() } else { b.img.clone() },
        z_index: if t < 0.5 { a.z_index } else { b.z_index },
        fixed_in_frame: if t < 0.5 {
            a.fixed_in_frame
        } else {
            b.fixed_in_frame
        },
    }
}

fn lerp_float2(a: Float2, b: Float2, t: f64) -> Float2 {
    a + (b - a) * t as f32
}

fn lerp_float3(a: Float3, b: Float3, t: f64) -> Float3 {
    a + (b - a) * t as f32
}

fn lerp_float4(a: Float4, b: Float4, t: f64) -> Float4 {
    a + (b - a) * t as f32
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
