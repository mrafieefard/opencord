//! Ids cross to JavaScript as decimal strings: an `i64` would become a
//! `bigint`, which can be neither a React key nor a route parameter.

use crate::MobileError;

/// The decimal form of an id.
pub fn id(value: i64) -> String {
    value.to_string()
}

/// An id back from its decimal form; anything else is `InvalidInput`.
pub fn parse_id(text: &str) -> Result<i64, MobileError> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(not_an_id(text));
    }
    text.parse().map_err(|_| not_an_id(text))
}

fn not_an_id(text: &str) -> MobileError {
    MobileError::InvalidInput {
        message: format!("not an id: {text:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_as_decimal_strings() {
        assert_eq!(parse_id(&id(i64::MAX)).unwrap(), i64::MAX);
        assert_eq!(parse_id(&id(i64::MIN)).unwrap(), i64::MIN);
        assert_eq!(parse_id("42").unwrap(), 42);
        for wrong in ["", "abc", " 1", "1.0", "+1", "-", "99999999999999999999"] {
            match parse_id(wrong) {
                Err(MobileError::InvalidInput { message }) => {
                    assert!(message.starts_with("not an id"), "{wrong:?}: {message}");
                }
                other => panic!("{wrong:?} gave {other:?}"),
            }
        }
    }
}
