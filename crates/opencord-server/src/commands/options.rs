//! A command's options as sent, checked against its spec (§17.1): every
//! name known, required ones there, values of the right kind.

use std::collections::HashMap;

use opencord_proto::v1 as proto;

use super::Spec;
use crate::error::ApiError;
use proto::CommandOptionKind as Kind;

/// Option values are at most this long.
pub const VALUE_MAX: usize = 400;

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::new(proto::ErrorCode::CommandInvalidOptions, message.into())
}

/// The options of one run, checked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    values: HashMap<String, String>,
}

impl Options {
    pub fn parse(spec: &Spec, sent: &[proto::CommandOptionValue]) -> Result<Self, ApiError> {
        let mut values = HashMap::new();
        for option in sent {
            let Some(known) = spec.option(&option.name) else {
                return Err(invalid(format!(
                    "/{} has no option {}",
                    spec.name, option.name
                )));
            };
            let value = option.value.trim();
            if value.is_empty() {
                continue;
            }
            if value.chars().count() > VALUE_MAX {
                return Err(invalid(format!("{} is too long", known.name)));
            }
            match known.kind {
                Kind::Integer => {
                    let number: i64 = value
                        .parse()
                        .map_err(|_| invalid(format!("{} is a whole number", known.name)))?;
                    let (min, max) = known.range;
                    if (min, max) != (0, 0) && !(min..=max).contains(&number) {
                        return Err(invalid(format!("{} is from {min} to {max}", known.name)));
                    }
                }
                Kind::Boolean => {
                    yes_or_no(value)
                        .ok_or_else(|| invalid(format!("{} is yes or no", known.name)))?;
                }
                Kind::Choice => {
                    if !known.choices.iter().any(|(_, choice)| *choice == value) {
                        let all: Vec<&str> =
                            known.choices.iter().map(|(_, choice)| *choice).collect();
                        return Err(invalid(format!(
                            "{} is one of {}",
                            known.name,
                            all.join(", ")
                        )));
                    }
                }
                Kind::User | Kind::Channel => {
                    value
                        .parse::<i64>()
                        .map_err(|_| invalid(format!("{} isn't one", known.name)))?;
                }
                Kind::Text | Kind::Unspecified => {}
            }
            if values
                .insert(known.name.to_owned(), value.to_owned())
                .is_some()
            {
                return Err(invalid(format!("{} is given twice", known.name)));
            }
        }
        for option in spec.options.iter().filter(|option| option.required) {
            if !values.contains_key(option.name) {
                return Err(invalid(format!("/{} needs {}", spec.name, option.name)));
            }
        }
        Ok(Self { values })
    }

    pub fn text(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    pub fn integer(&self, name: &str) -> Option<i64> {
        self.text(name)?.parse().ok()
    }

    pub fn boolean(&self, name: &str) -> bool {
        self.text(name).and_then(yes_or_no).unwrap_or(false)
    }
}

fn yes_or_no(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "yes" | "true" | "1" | "on" => Some(true),
        "no" | "false" | "0" | "off" => Some(false),
        _ => None,
    }
}

/// "1:30", "90" or "1:02:03" in milliseconds.
pub fn parse_time(text: &str) -> Option<u64> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut seconds = 0u64;
    for (index, part) in parts.iter().enumerate() {
        let value: u64 = part.parse().ok()?;
        if index > 0 && value >= 60 {
            return None;
        }
        seconds = seconds.checked_mul(60)?.checked_add(value)?;
    }
    seconds.checked_mul(1000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::find;

    fn sent(pairs: &[(&str, &str)]) -> Vec<proto::CommandOptionValue> {
        pairs
            .iter()
            .map(|(name, value)| proto::CommandOptionValue {
                name: (*name).to_owned(),
                value: (*value).to_owned(),
            })
            .collect()
    }

    #[test]
    fn options_are_checked_against_the_spec() {
        let play = find("play").unwrap();
        let ok = Options::parse(play, &sent(&[("query", " lofi "), ("next", "yes")])).unwrap();
        assert_eq!(ok.text("query"), Some("lofi"));
        assert!(ok.boolean("next"));
        for bad in [
            sent(&[]),
            sent(&[("query", "a"), ("loud", "yes")]),
            sent(&[("query", "a"), ("next", "maybe")]),
            sent(&[("query", "a"), ("query", "b")]),
            sent(&[("query", &"a".repeat(401))]),
        ] {
            let error = Options::parse(play, &bad).unwrap_err();
            assert_eq!(
                error.code,
                proto::ErrorCode::CommandInvalidOptions,
                "{bad:?}"
            );
        }
        let repeat = find("repeat").unwrap();
        assert!(Options::parse(repeat, &sent(&[("mode", "queue")])).is_ok());
        assert!(Options::parse(repeat, &sent(&[("mode", "forever")])).is_err());
        let remove = find("remove").unwrap();
        assert!(Options::parse(remove, &sent(&[("position", "0")])).is_err());
        assert!(Options::parse(remove, &sent(&[("position", "two")])).is_err());
        assert_eq!(
            Options::parse(remove, &sent(&[("position", "2")]))
                .unwrap()
                .integer("position"),
            Some(2)
        );
    }

    #[test]
    fn times_read_as_people_write_them() {
        assert_eq!(parse_time("1:30"), Some(90_000));
        assert_eq!(parse_time("90"), Some(90_000));
        assert_eq!(parse_time("1:02:03"), Some(3_723_000));
        for bad in ["", "1:75", "a", "1:2:3:4", "-5"] {
            assert_eq!(parse_time(bad), None, "{bad}");
        }
    }
}
