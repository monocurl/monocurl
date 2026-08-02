use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::with_heap,
    value::{Value, container::List},
};
use stdlib_macros::stdlib_func;

const MAX_DECIMAL_PLACES: usize = 64;

fn format_text_tag(tag: &[isize], target: &str) -> String {
    let tag = tag
        .iter()
        .map(isize::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!("\\text_tag{{{tag}}}{{{target}}}")
}

async fn read_text_tag_component(
    executor: &mut Executor,
    value: Value,
) -> Result<isize, ExecutorError> {
    match value.elide_wrappers_rec(executor).await? {
        Value::Integer(value) => isize::try_from(value).map_err(|_| {
            ExecutorError::invalid_invocation("text tag component is out of range for isize")
        }),
        other => Err(ExecutorError::type_error("int", other.type_name())),
    }
}

async fn read_text_tag(executor: &mut Executor, value: Value) -> Result<Vec<isize>, ExecutorError> {
    let resolved = value.elide_wrappers_rec(executor).await?;
    Ok(match resolved {
        Value::Integer(value) => vec![isize::try_from(value).map_err(|_| {
            ExecutorError::invalid_invocation("text tag component is out of range for isize")
        })?],
        Value::List(list) => read_text_tag_list(executor, list).await?,
        other => return Err(ExecutorError::type_error("int / list", other.type_name())),
    })
}

async fn read_text_tag_list(
    executor: &mut Executor,
    list: List,
) -> Result<Vec<isize>, ExecutorError> {
    let mut out = Vec::with_capacity(list.len());
    for key in list.elements() {
        let value = with_heap(|h| h.get(key.key()).clone());
        out.push(read_text_tag_component(executor, value).await?);
    }
    Ok(out)
}

fn read_decimal_places(
    executor: &Executor,
    stack_idx: usize,
) -> Result<Option<usize>, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(-1)
        .clone()
        .elide_lvalue()
    {
        Value::Nil => Ok(None),
        Value::Integer(value) if (0..=MAX_DECIMAL_PLACES as i64).contains(&value) => {
            Ok(Some(value as usize))
        }
        Value::Float(value)
            if value.is_finite()
                && value.fract() == 0.0
                && (0.0..=MAX_DECIMAL_PLACES as f64).contains(&value) =>
        {
            Ok(Some(value as usize))
        }
        Value::Integer(_) | Value::Float(_) => Err(ExecutorError::InvalidArgument {
            arg: "decimal_places",
            message: "must be nil or a non-negative integer at most 64",
        }),
        other => Err(ExecutorError::type_error_for(
            "nil / non-negative int",
            other.type_name(),
            "decimal_places",
        )),
    }
}

#[stdlib_func]
pub async fn to_string(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let decimal_places = read_decimal_places(executor, stack_idx)?;
    let value = executor.state.stack(stack_idx).read_at(-2).clone();
    let resolved = value.clone().elide_wrappers_rec(executor).await?;
    let s = match (resolved, decimal_places) {
        (Value::Integer(value), decimal_places) => {
            text::format_number(value as f64, decimal_places, false)
                .map_err(|error| ExecutorError::invalid_invocation(error.to_string()))?
        }
        (Value::Float(value), decimal_places) => text::format_number(value, decimal_places, false)
            .map_err(|error| ExecutorError::invalid_invocation(error.to_string()))?,
        (other, Some(_)) => {
            return Err(ExecutorError::type_error_for(
                "number",
                other.type_name(),
                "x",
            ));
        }
        (_, None) => {
            crate::stringify_value(executor, value)
                .await
                .map_err(|error| match error {
                    ExecutorError::TypeError { got, .. } => {
                        ExecutorError::type_error(crate::STRING_COMPATIBLE_DESC, got)
                    }
                    other => other,
                })?
        }
    };
    Ok(Value::String(s.into()))
}

#[stdlib_func]
pub async fn text_tag_encode(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let target = crate::stringify_value(
        executor,
        executor.state.stack(stack_idx).read_at(-2).clone(),
    )
    .await
    .map_err(|error| match error {
        ExecutorError::TypeError { got, .. } => {
            ExecutorError::type_error_for(crate::STRING_COMPATIBLE_DESC, got, "target")
        }
        other => other,
    })?;
    let tag = read_text_tag(
        executor,
        executor.state.stack(stack_idx).read_at(-1).clone(),
    )
    .await?;
    Ok(Value::String(format_text_tag(&tag, &target).into()))
}

#[stdlib_func]
pub async fn to_int(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_cached_wrappers_rec()
    {
        Value::Integer(n) => Ok(Value::Integer(n)),
        Value::Float(f) => Ok(Value::Integer(f as i64)),
        Value::String(s) => s.trim().parse::<i64>().map(Value::Integer).map_err(|_| {
            ExecutorError::InvalidArgument {
                arg: "x",
                message: "cannot parse as int",
            }
        }),
        other => Err(ExecutorError::type_error(
            "number / string",
            other.type_name(),
        )),
    }
}

#[stdlib_func]
pub async fn to_float(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_cached_wrappers_rec()
    {
        Value::Integer(n) => Ok(Value::Float(n as f64)),
        Value::Float(f) => Ok(Value::Float(f)),
        Value::String(s) => {
            s.trim()
                .parse::<f64>()
                .map(Value::Float)
                .map_err(|_| ExecutorError::InvalidArgument {
                    arg: "x",
                    message: "cannot parse as float",
                })
        }
        other => Err(ExecutorError::type_error(
            "number / string",
            other.type_name(),
        )),
    }
}

#[stdlib_func]
pub async fn real(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_wrappers_rec(executor)
        .await?
    {
        Value::Integer(n) => Ok(Value::Integer(n)),
        Value::Float(f) => Ok(Value::Float(f)),
        Value::Complex { re, .. } => Ok(Value::Float(re)),
        other => Err(ExecutorError::type_error_for(
            "number",
            other.type_name(),
            "x",
        )),
    }
}

#[stdlib_func]
pub async fn imag(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_wrappers_rec(executor)
        .await?
    {
        Value::Integer(_) | Value::Float(_) => Ok(Value::Integer(0)),
        Value::Complex { im, .. } => Ok(Value::Float(im)),
        other => Err(ExecutorError::type_error_for(
            "number",
            other.type_name(),
            "x",
        )),
    }
}
