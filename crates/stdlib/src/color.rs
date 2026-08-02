use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::VRc,
    value::{Value, container::List},
};
use stdlib_macros::stdlib_func;

use crate::{STRING_COMPATIBLE_DESC, read_float, stringify_value};

fn parse_hex_color(value: &str) -> Result<[f64; 4], String> {
    let value = value.strip_prefix('#').unwrap_or(value);
    if !matches!(value.len(), 6 | 8) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("expected a 6- or 8-digit hexadecimal color, such as #009ee0".into());
    }

    let channel = |start| u8::from_str_radix(&value[start..start + 2], 16).unwrap() as f64 / 255.0;
    Ok([
        channel(0),
        channel(2),
        channel(4),
        if value.len() == 8 { channel(6) } else { 1.0 },
    ])
}

#[stdlib_func]
pub async fn hex(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let value = executor.state.stack(stack_idx).read_at(-1).clone();
    let value = stringify_value(executor, value)
        .await
        .map_err(|error| match error {
            ExecutorError::TypeError { got, .. } => {
                ExecutorError::type_error_for(STRING_COMPATIBLE_DESC, got, "value")
            }
            other => other,
        })?;
    let color = parse_hex_color(&value).map_err(ExecutorError::invalid_operation)?;

    Ok(Value::List(List::new_with(
        color
            .into_iter()
            .map(|channel| VRc::new(Value::Float(channel))),
    )))
}

#[stdlib_func]
pub async fn hsv(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let h = read_float(executor, stack_idx, -4, "h")?;
    let s = read_float(executor, stack_idx, -3, "s")?;
    let v = read_float(executor, stack_idx, -2, "v")?;
    let a = read_float(executor, stack_idx, -1, "a")?;

    let h = h.rem_euclid(1.0) * 6.0;
    let c = v * s;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let m = v - c;

    let (r, g, b) = match h as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    Ok(Value::List(List::new_with(vec![
        VRc::new(Value::Float(r + m)),
        VRc::new(Value::Float(g + m)),
        VRc::new(Value::Float(b + m)),
        VRc::new(Value::Float(a)),
    ])))
}

#[cfg(test)]
mod tests {
    use super::parse_hex_color;

    #[test]
    fn parses_rgb_hex_with_opaque_alpha() {
        assert_eq!(
            parse_hex_color("#009ee0").unwrap(),
            [0.0, 158.0 / 255.0, 224.0 / 255.0, 1.0]
        );
    }

    #[test]
    fn parses_rgba_hex() {
        assert_eq!(
            parse_hex_color("009ee080").unwrap(),
            [0.0, 158.0 / 255.0, 224.0 / 255.0, 128.0 / 255.0]
        );
    }

    #[test]
    fn rejects_invalid_hex() {
        assert!(parse_hex_color("#09e").is_err());
        assert!(parse_hex_color("#009eez").is_err());
    }
}
