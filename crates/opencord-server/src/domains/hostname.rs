//! Domain names as the owner types them, made into what DNS and
//! certificates use (Phase 3 plan §6.1): lowercase, internationalised names
//! in punycode, a public name with a dot in it.

use std::net::IpAddr;

use hickory_proto::rr::Name;

const MAX_NAME: usize = 253;
const MAX_LABEL: usize = 63;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HostnameError {
    #[error("enter a domain name, like chat.example.com")]
    Empty,
    #[error("{0} is an IP address; add a domain name that points to it instead")]
    IpAddress(String),
    #[error("{0} is not a domain name on the internet")]
    NotPublic(String),
    #[error("{0} is not a valid domain name")]
    Invalid(String),
}

/// `input` as the name to check and certify: trimmed, without a scheme,
/// port, path or final dot, in lowercase punycode.
pub fn normalize(input: &str) -> Result<String, HostnameError> {
    let trimmed = input.trim();
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .or_else(|| trimmed.strip_prefix("opencord://"))
        .unwrap_or(trimmed);
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    // A port the owner pasted along goes; IPv6 addresses are refused below.
    let host = match host.rsplit_once(':') {
        Some((name, port)) if !name.contains(':') && port.parse::<u16>().is_ok() => name,
        _ => host,
    };
    let host = host.trim_end_matches('.');
    if host.is_empty() {
        return Err(HostnameError::Empty);
    }
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if bare.parse::<IpAddr>().is_ok() {
        return Err(HostnameError::IpAddress(bare.to_owned()));
    }
    let ascii = Name::from_utf8(host)
        .map_err(|_| HostnameError::Invalid(host.to_owned()))?
        .to_lowercase()
        .to_ascii();
    let ascii = ascii.trim_end_matches('.').to_owned();
    let labels: Vec<&str> = ascii.split('.').collect();
    let valid = ascii.len() <= MAX_NAME
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= MAX_LABEL
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        });
    if !valid {
        return Err(HostnameError::Invalid(host.to_owned()));
    }
    let top = labels.last().copied().unwrap_or_default();
    let local = [
        "localhost",
        "local",
        "internal",
        "lan",
        "home",
        "test",
        "invalid",
    ];
    if labels.len() < 2 || local.contains(&top) || top.bytes().all(|b| b.is_ascii_digit()) {
        return Err(HostnameError::NotPublic(ascii));
    }
    Ok(ascii)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_cleaned_up() {
        assert_eq!(
            normalize(" Chat.Example.COM. ").unwrap(),
            "chat.example.com"
        );
        assert_eq!(
            normalize("https://chat.example.com:7710/invite/x").unwrap(),
            "chat.example.com"
        );
        assert_eq!(
            normalize("chat.example.com:443").unwrap(),
            "chat.example.com"
        );
    }

    #[test]
    fn international_names_become_punycode() {
        assert_eq!(
            normalize("bücher.example").unwrap(),
            "xn--bcher-kva.example"
        );
        assert_eq!(normalize("ЧАТ.рф").unwrap(), "xn--80a0bn.xn--p1ai");
    }

    #[test]
    fn what_is_not_a_public_name_is_refused() {
        assert_eq!(normalize("  "), Err(HostnameError::Empty));
        assert!(matches!(
            normalize("203.0.113.10"),
            Err(HostnameError::IpAddress(_))
        ));
        assert!(matches!(
            normalize("[2001:db8::1]"),
            Err(HostnameError::IpAddress(_))
        ));
        assert!(matches!(
            normalize("localhost"),
            Err(HostnameError::NotPublic(_))
        ));
        assert!(matches!(
            normalize("server.local"),
            Err(HostnameError::NotPublic(_))
        ));
        assert!(matches!(
            normalize("chat"),
            Err(HostnameError::NotPublic(_))
        ));
        assert!(matches!(
            normalize("-bad.example.com"),
            Err(HostnameError::Invalid(_))
        ));
        assert!(matches!(
            normalize("under_score.example.com"),
            Err(HostnameError::Invalid(_))
        ));
        assert!(matches!(
            normalize(&format!("{}.example.com", "a".repeat(64))),
            Err(HostnameError::Invalid(_))
        ));
    }
}
