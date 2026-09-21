/// snake_case a PascalCase (or already-snake) identifier: `Wobble` → `wobble`,
/// `RotateAbout` → `rotate_about`.
pub fn to_snake_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 4);
    let mut prev_lower_or_digit = false;
    for ch in value.chars() {
        if ch.is_uppercase() {
            if prev_lower_or_digit {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
            prev_lower_or_digit = false;
        } else {
            out.push(ch);
            prev_lower_or_digit = ch.is_lowercase() || ch.is_ascii_digit();
        }
    }
    out
}

/// snake_case → PascalCase: `wobble` → `Wobble`, `rotate_about` → `RotateAbout`.
pub fn to_pascal_case(value: &str) -> String {
    value
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .into_iter()
                .flat_map(char::to_uppercase)
                .chain(chars)
                .collect::<String>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_examples() {
        assert_eq!(to_snake_case("Wobble"), "wobble");
        assert_eq!(to_snake_case("RotateAbout"), "rotate_about");
        assert_eq!(to_snake_case("Squash"), "squash");
    }

    #[test]
    fn pascal_case_examples() {
        assert_eq!(to_pascal_case("wobble"), "Wobble");
        assert_eq!(to_pascal_case("rotate_about"), "RotateAbout");
    }
}
