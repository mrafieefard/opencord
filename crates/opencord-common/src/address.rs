//! Server addresses, certificate fingerprints and invite links
//! (`opencord://host:port/invite/CODE#fp=SHA256HEX`).

use std::fmt;

pub const DEFAULT_PORT: u16 = 7710;
pub const INVITE_CODE_MAX_LEN: usize = 32;
/// SHA-256 of the server's DER-encoded TLS certificate.
pub type Fingerprint = [u8; 32];

const SCHEME: &str = "opencord://";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AddressError {
    #[error("the address has no host")]
    MissingHost,
    #[error("the host is not valid; IPv6 addresses need brackets, like [::1]:7710")]
    InvalidHost,
    #[error("the port is not a number between 1 and 65535")]
    InvalidPort,
    #[error("invite links start with opencord://")]
    InvalidScheme,
    #[error("the link is not an invite link")]
    NotAnInvite,
    #[error("the invite code must be 1 to 32 letters or digits")]
    InvalidCode,
    #[error("the fingerprint must be 64 hexadecimal characters")]
    InvalidFingerprint,
}

/// `host` or `host:port`; IPv6 hosts use brackets (`[::1]:7710`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerAddress {
    /// Lowercase, without brackets.
    pub host: String,
    pub port: u16,
}

impl ServerAddress {
    pub fn parse(input: &str) -> Result<Self, AddressError> {
        let input = input.trim();
        let (host, port) = match input.strip_prefix('[') {
            Some(bracketed) => {
                let (host, after) = bracketed.split_once(']').ok_or(AddressError::InvalidHost)?;
                let port = match after {
                    "" => None,
                    _ => Some(after.strip_prefix(':').ok_or(AddressError::InvalidPort)?),
                };
                (host, port)
            }
            None => match input.split_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (input, None),
            },
        };
        if host.is_empty() {
            return Err(AddressError::MissingHost);
        }
        if host
            .chars()
            .any(|c| c.is_whitespace() || "/?#@[]".contains(c))
        {
            return Err(AddressError::InvalidHost);
        }
        let port = match port {
            None => DEFAULT_PORT,
            Some(port) => port
                .parse::<u16>()
                .ok()
                .filter(|port| *port != 0)
                .ok_or(AddressError::InvalidPort)?,
        };
        Ok(Self {
            host: host.to_ascii_lowercase(),
            port,
        })
    }
}

impl fmt::Display for ServerAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.host.contains(':') {
            write!(f, "[{}]:{}", self.host, self.port)
        } else {
            write!(f, "{}:{}", self.host, self.port)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteLink {
    pub address: ServerAddress,
    pub code: String,
    pub fingerprint: Option<Fingerprint>,
}

impl InviteLink {
    pub fn parse(input: &str) -> Result<Self, AddressError> {
        let rest = input
            .trim()
            .strip_prefix(SCHEME)
            .ok_or(AddressError::InvalidScheme)?;
        let (main, fragment) = match rest.split_once('#') {
            Some((main, fragment)) => (main, Some(fragment)),
            None => (rest, None),
        };
        let (authority, path) = main.split_once('/').ok_or(AddressError::NotAnInvite)?;
        let code = path
            .strip_prefix("invite/")
            .ok_or(AddressError::NotAnInvite)?;
        if !is_valid_invite_code(code) {
            return Err(AddressError::InvalidCode);
        }
        let fingerprint = fragment
            .map(|fragment| {
                fragment
                    .strip_prefix("fp=")
                    .ok_or(AddressError::InvalidFingerprint)
                    .and_then(parse_fingerprint)
            })
            .transpose()?;
        Ok(Self {
            address: ServerAddress::parse(authority)?,
            code: code.to_owned(),
            fingerprint,
        })
    }
}

impl fmt::Display for InviteLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{SCHEME}{}/invite/{}", self.address, self.code)?;
        match &self.fingerprint {
            Some(fingerprint) => write!(f, "#fp={}", format_fingerprint(fingerprint)),
            None => Ok(()),
        }
    }
}

/// 1 to 32 ASCII letters or digits.
pub fn is_valid_invite_code(code: &str) -> bool {
    (1..=INVITE_CODE_MAX_LEN).contains(&code.len())
        && code.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

/// Lowercase hex.
pub fn format_fingerprint(fingerprint: &Fingerprint) -> String {
    hex::encode(fingerprint)
}

/// Accepts upper- or lowercase hex, optionally separated by colons.
pub fn parse_fingerprint(input: &str) -> Result<Fingerprint, AddressError> {
    let digits: String = input.trim().chars().filter(|c| *c != ':').collect();
    let mut fingerprint = [0; 32];
    hex::decode_to_slice(digits, &mut fingerprint).map_err(|_| AddressError::InvalidFingerprint)?;
    Ok(fingerprint)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FP_HEX: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    fn address(host: &str, port: u16) -> ServerAddress {
        ServerAddress {
            host: host.to_owned(),
            port,
        }
    }

    #[test]
    fn parses_host_and_port() {
        assert_eq!(
            ServerAddress::parse("Chat.Example.com:8000").unwrap(),
            address("chat.example.com", 8000)
        );
        assert_eq!(
            ServerAddress::parse(" 10.0.0.5 ").unwrap(),
            address("10.0.0.5", DEFAULT_PORT)
        );
    }

    #[test]
    fn parses_bracketed_ipv6() {
        assert_eq!(
            ServerAddress::parse("[::1]:9000").unwrap(),
            address("::1", 9000)
        );
        assert_eq!(
            ServerAddress::parse("[fe80::1]").unwrap(),
            address("fe80::1", DEFAULT_PORT)
        );
        assert_eq!(address("::1", 9000).to_string(), "[::1]:9000");
    }

    #[test]
    fn rejects_bad_addresses() {
        assert_eq!(ServerAddress::parse(""), Err(AddressError::MissingHost));
        assert_eq!(
            ServerAddress::parse(":7710"),
            Err(AddressError::MissingHost)
        );
        assert_eq!(
            ServerAddress::parse("host:0"),
            Err(AddressError::InvalidPort)
        );
        assert_eq!(
            ServerAddress::parse("host:99999"),
            Err(AddressError::InvalidPort)
        );
        assert_eq!(
            ServerAddress::parse("host:abc"),
            Err(AddressError::InvalidPort)
        );
    }

    #[test]
    fn parses_a_full_invite_link() {
        let link = InviteLink::parse(&format!(
            "opencord://chat.example.com:7711/invite/AbC123#fp={FP_HEX}"
        ))
        .unwrap();

        assert_eq!(link.address, address("chat.example.com", 7711));
        assert_eq!(link.code, "AbC123");
        assert_eq!(link.fingerprint, Some(parse_fingerprint(FP_HEX).unwrap()));
    }

    #[test]
    fn invite_link_round_trips() {
        let link = InviteLink {
            address: address("::1", 7710),
            code: "xyz".to_owned(),
            fingerprint: Some([0xab; 32]),
        };

        let text = link.to_string();

        assert_eq!(
            text,
            format!("opencord://[::1]:7710/invite/xyz#fp={}", "ab".repeat(32))
        );
        assert_eq!(InviteLink::parse(&text).unwrap(), link);
    }

    #[test]
    fn fingerprint_is_optional_and_case_insensitive() {
        let without = InviteLink::parse("opencord://host/invite/code").unwrap();
        let upper = InviteLink::parse(&format!(
            "opencord://host/invite/code#fp={}",
            FP_HEX.to_uppercase()
        ))
        .unwrap();

        assert_eq!(without.fingerprint, None);
        assert_eq!(without.address.port, DEFAULT_PORT);
        assert_eq!(upper.fingerprint, Some(parse_fingerprint(FP_HEX).unwrap()));
    }

    #[test]
    fn rejects_bad_invite_links() {
        assert_eq!(
            InviteLink::parse("https://host/invite/code"),
            Err(AddressError::InvalidScheme)
        );
        assert_eq!(
            InviteLink::parse("opencord://host/join/code"),
            Err(AddressError::NotAnInvite)
        );
        assert_eq!(
            InviteLink::parse("opencord://host/invite/bad-code!"),
            Err(AddressError::InvalidCode)
        );
        assert_eq!(
            InviteLink::parse(&format!("opencord://host/invite/{}", "a".repeat(33))),
            Err(AddressError::InvalidCode)
        );
        assert_eq!(
            InviteLink::parse("opencord://host/invite/code#fp=abc"),
            Err(AddressError::InvalidFingerprint)
        );
    }

    #[test]
    fn fingerprints_format_as_lowercase_hex_and_accept_colons() {
        let fingerprint = parse_fingerprint(FP_HEX).unwrap();
        let with_colons = FP_HEX
            .as_bytes()
            .chunks(2)
            .map(|pair| std::str::from_utf8(pair).unwrap())
            .collect::<Vec<_>>()
            .join(":");

        assert_eq!(format_fingerprint(&fingerprint), FP_HEX);
        assert_eq!(parse_fingerprint(&with_colons), Ok(fingerprint));
        assert_eq!(
            parse_fingerprint("zz"),
            Err(AddressError::InvalidFingerprint)
        );
    }
}
