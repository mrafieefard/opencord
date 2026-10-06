//! Normalizes and validates user input. Lengths count Unicode scalar values.

use crate::channel::ChannelKind;
use crate::limits::{
    CHANNEL_NAME_MAX_CHARS, FETCH_LIMIT_DEFAULT, FETCH_LIMIT_MAX, MESSAGE_MAX_CHARS,
    NAME_MAX_CHARS, REASON_MAX_CHARS, ROLE_NAME_MAX_CHARS, SERVER_DESCRIPTION_MAX_CHARS,
    SERVER_NAME_MAX_CHARS, TOPIC_MAX_CHARS,
};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} must be at most {max} characters")]
    TooLong { field: &'static str, max: usize },
    #[error("{field} must not contain control characters")]
    ControlCharacters { field: &'static str },
}

/// Trimmed message text. Line breaks are kept.
pub fn message_content(input: &str) -> Result<String, ValidationError> {
    let trimmed = input.trim();
    require_non_empty("message", trimmed)?;
    require_max_chars("message", trimmed, MESSAGE_MAX_CHARS)?;
    Ok(trimmed.to_owned())
}

pub fn display_name(input: &str) -> Result<String, ValidationError> {
    single_line_name("display name", input, NAME_MAX_CHARS)
}

/// `None` clears the nickname.
pub fn nickname(input: &str) -> Result<Option<String>, ValidationError> {
    if input.trim().is_empty() {
        return Ok(None);
    }
    single_line_name("nickname", input, NAME_MAX_CHARS).map(Some)
}

/// Text channel names are normalized to lowercase-kebab (`My Chat!` becomes
/// `my-chat`); other kinds are only trimmed.
pub fn channel_name(kind: ChannelKind, input: &str) -> Result<String, ValidationError> {
    match kind {
        ChannelKind::Text => {
            single_line_name("channel name", &kebab_case(input), CHANNEL_NAME_MAX_CHARS)
        }
        ChannelKind::Voice | ChannelKind::Category => {
            single_line_name("channel name", input, CHANNEL_NAME_MAX_CHARS)
        }
    }
}

pub fn role_name(input: &str) -> Result<String, ValidationError> {
    single_line_name("role name", input, ROLE_NAME_MAX_CHARS)
}

/// `None` clears the topic.
pub fn topic(input: &str) -> Result<Option<String>, ValidationError> {
    optional_text("topic", input, TOPIC_MAX_CHARS)
}

pub fn server_name(input: &str) -> Result<String, ValidationError> {
    single_line_name("server name", input, SERVER_NAME_MAX_CHARS)
}

/// May be empty.
pub fn server_description(input: &str) -> Result<String, ValidationError> {
    let trimmed = input.trim();
    require_max_chars("server description", trimmed, SERVER_DESCRIPTION_MAX_CHARS)?;
    Ok(trimmed.to_owned())
}

/// Kick or ban reason. `None` when empty.
pub fn reason(input: &str) -> Result<Option<String>, ValidationError> {
    optional_text("reason", input, REASON_MAX_CHARS)
}

/// 0 means the default; anything above the maximum is capped.
pub fn fetch_limit(requested: u32) -> u32 {
    if requested == 0 {
        FETCH_LIMIT_DEFAULT
    } else {
        requested.min(FETCH_LIMIT_MAX)
    }
}

fn single_line_name(
    field: &'static str,
    input: &str,
    max: usize,
) -> Result<String, ValidationError> {
    let trimmed = input.trim();
    require_non_empty(field, trimmed)?;
    if trimmed.chars().any(char::is_control) {
        return Err(ValidationError::ControlCharacters { field });
    }
    require_max_chars(field, trimmed, max)?;
    Ok(trimmed.to_owned())
}

fn optional_text(
    field: &'static str,
    input: &str,
    max: usize,
) -> Result<Option<String>, ValidationError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    require_max_chars(field, trimmed, max)?;
    Ok(Some(trimmed.to_owned()))
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), ValidationError> {
    if value.is_empty() {
        Err(ValidationError::Empty { field })
    } else {
        Ok(())
    }
}

fn require_max_chars(field: &'static str, value: &str, max: usize) -> Result<(), ValidationError> {
    if value.chars().count() > max {
        Err(ValidationError::TooLong { field, max })
    } else {
        Ok(())
    }
}

/// Lowercases, joins words with single dashes and drops everything except
/// letters, digits and underscores.
fn kebab_case(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut pending_dash = false;
    for c in input.trim().chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() || c == '_' {
            if pending_dash && !output.is_empty() {
                output.push('-');
            }
            pending_dash = false;
            output.push(c);
        } else if c.is_whitespace() || c == '-' {
            pending_dash = true;
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_is_trimmed_and_keeps_line_breaks() {
        assert_eq!(message_content("  hi\n there \n").unwrap(), "hi\n there");
    }

    #[test]
    fn message_must_not_be_blank() {
        assert_eq!(
            message_content(" \n\t "),
            Err(ValidationError::Empty { field: "message" })
        );
    }

    #[test]
    fn message_length_counts_characters_not_bytes() {
        assert!(message_content(&"é".repeat(MESSAGE_MAX_CHARS)).is_ok());
        assert_eq!(
            message_content(&"a".repeat(MESSAGE_MAX_CHARS + 1)),
            Err(ValidationError::TooLong {
                field: "message",
                max: MESSAGE_MAX_CHARS
            })
        );
    }

    #[test]
    fn display_name_is_trimmed_and_limited() {
        assert_eq!(display_name("  Ana  ").unwrap(), "Ana");
        assert!(display_name(&"ß".repeat(NAME_MAX_CHARS)).is_ok());
        assert_eq!(
            display_name(&"a".repeat(NAME_MAX_CHARS + 1)),
            Err(ValidationError::TooLong {
                field: "display name",
                max: NAME_MAX_CHARS
            })
        );
        assert_eq!(
            display_name("   "),
            Err(ValidationError::Empty {
                field: "display name"
            })
        );
    }

    #[test]
    fn names_reject_control_characters() {
        assert_eq!(
            display_name("a\u{7}b"),
            Err(ValidationError::ControlCharacters {
                field: "display name"
            })
        );
        assert_eq!(
            role_name("mod\nerator"),
            Err(ValidationError::ControlCharacters { field: "role name" })
        );
    }

    #[test]
    fn empty_nickname_clears_it() {
        assert_eq!(nickname("  "), Ok(None));
        assert_eq!(nickname(" Bob ").unwrap(), Some("Bob".to_owned()));
    }

    #[test]
    fn text_channel_names_become_lowercase_kebab() {
        let cases = [
            ("General", "general"),
            ("  My Cool  Channel! ", "my-cool-channel"),
            ("rust_lang", "rust_lang"),
            ("Ünïcode Chat", "ünïcode-chat"),
            ("a - b", "a-b"),
        ];
        for (input, expected) in cases {
            assert_eq!(channel_name(ChannelKind::Text, input).unwrap(), expected);
        }
    }

    #[test]
    fn text_channel_name_must_keep_some_characters() {
        assert_eq!(
            channel_name(ChannelKind::Text, " !?- "),
            Err(ValidationError::Empty {
                field: "channel name"
            })
        );
    }

    #[test]
    fn other_channel_names_are_only_trimmed() {
        assert_eq!(
            channel_name(ChannelKind::Voice, "  Lounge 1 ").unwrap(),
            "Lounge 1"
        );
        assert_eq!(
            channel_name(ChannelKind::Category, "Text Channels").unwrap(),
            "Text Channels"
        );
    }

    #[test]
    fn channel_names_are_limited() {
        assert_eq!(
            channel_name(ChannelKind::Voice, &"a".repeat(CHANNEL_NAME_MAX_CHARS + 1)),
            Err(ValidationError::TooLong {
                field: "channel name",
                max: CHANNEL_NAME_MAX_CHARS
            })
        );
    }

    #[test]
    fn empty_topic_clears_it() {
        assert_eq!(topic(""), Ok(None));
        assert_eq!(
            topic(" news \n links ").unwrap(),
            Some("news \n links".to_owned())
        );
        assert!(topic(&"a".repeat(TOPIC_MAX_CHARS + 1)).is_err());
    }

    #[test]
    fn server_fields_are_validated() {
        assert_eq!(server_name(" Home ").unwrap(), "Home");
        assert!(server_name("").is_err());
        assert_eq!(server_description("").unwrap(), "");
        assert!(server_description(&"a".repeat(SERVER_DESCRIPTION_MAX_CHARS + 1)).is_err());
    }

    #[test]
    fn empty_reason_is_none() {
        assert_eq!(reason(" "), Ok(None));
        assert_eq!(reason("spam").unwrap(), Some("spam".to_owned()));
        assert!(reason(&"a".repeat(REASON_MAX_CHARS + 1)).is_err());
    }

    #[test]
    fn fetch_limit_defaults_and_caps() {
        assert_eq!(fetch_limit(0), FETCH_LIMIT_DEFAULT);
        assert_eq!(fetch_limit(10), 10);
        assert_eq!(fetch_limit(500), FETCH_LIMIT_MAX);
    }
}
