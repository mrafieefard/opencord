//! The last filter before the disk (D82): whatever the code didn't alias,
//! a third-party crate included, loses its URLs, addresses, domains,
//! e-mail addresses, home folder and anything shaped like a secret.

use std::borrow::Cow;
use std::net::IpAddr;
use std::sync::LazyLock;

use regex::{Captures, Regex};

use super::alias::Aliases;

/// Top-level domains beyond the two-letter country codes: `provider.read`
/// in a Dart message stays, `chat.example.dev` doesn't.
const TOP_LEVEL: &[&str] = &[
    "com", "net", "org", "info", "biz", "edu", "gov", "mil", "int", "io", "xyz", "dev", "app",
    "ai", "gg", "tv", "me", "cloud", "online", "site", "tech", "live", "link", "one", "top",
    "club", "pro", "name", "mobi", "asia", "store", "shop", "blog", "page", "host", "space",
    "world", "zone", "network", "digital", "services", "systems", "email", "chat", "social", "fun",
    "games", "run", "win", "bot", "local", "lan", "internal", "home", "arpa", "onion", "lol",
    "wtf", "art", "design", "media", "studio", "software", "website", "icu", "cyou",
];

/// Last labels that make a dotted word a file, not a domain.
const FILE_EXTENSIONS: &[&str] = &[
    "rs",
    "dart",
    "dll",
    "exe",
    "so",
    "dylib",
    "json",
    "db",
    "log",
    "txt",
    "toml",
    "yaml",
    "yml",
    "png",
    "jpg",
    "jpeg",
    "gif",
    "webp",
    "wav",
    "mp3",
    "ogg",
    "opus",
    "flac",
    "m4a",
    "mp4",
    "sys",
    "ini",
    "dat",
    "md",
    "html",
    "js",
    "plist",
    "framework",
    "dmg",
    "zip",
    "msi",
    "lnk",
    "sock",
    "conf",
    "cc",
    "cpp",
    "h",
    "mm",
    "swift",
    "kt",
    "java",
    "py",
    "sh",
    "tmp",
    "lock",
];

macro_rules! pattern {
    ($name:ident, $regex:expr) => {
        static $name: LazyLock<Regex> =
            LazyLock::new(|| Regex::new($regex).expect("a valid pattern"));
    };
}

pattern!(
    URL,
    r"(?i)\b([a-z][a-z0-9+.-]*)://(\[[0-9a-f:.]+\]|[^\s/:\x22'<>()\[\]?#@]+@?[^\s/:\x22'<>()\[\]?#]*)(:\d{1,5})?[^\s\x22'<>)\]]*"
);
pattern!(EMAIL, r"(?i)[\w.+-]+@(?:[a-z0-9-]+\.)+[a-z]{2,}\b");
pattern!(
    SECRET_FIELD,
    r"(?i)\b(bearer\s+|(?:token|password|passwd|secret|authorization|api_key|apikey)\s*[=:]\s*(?:bearer\s+)?)[^\s,;)\x22']+"
);
pattern!(
    IPV6,
    r"(^|[^\w:.])([0-9A-Fa-f]{0,4}(?::[0-9A-Fa-f]{0,4}){2,7})($|[^\w:])"
);
pattern!(IPV4, r"\b(\d{1,3}(?:\.\d{1,3}){3})\b");
// Lowercase only: `CoreError.notInitialized` and `RustRepository.start`
// are code, and hosts in errors come lowercase.
pattern!(
    DOMAIN,
    r"\b(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+([a-z]{2,63})\b"
);
pattern!(
    TOKEN,
    r"(\b(?:hash|sha256|event|id)=)?\b([A-Za-z0-9+/_-]{24,}={0,2})"
);

/// `text` with everything private replaced, through `aliases`; `home` is
/// the user's home folder.
pub(crate) fn line_with<'a>(
    aliases: &mut Aliases,
    text: &'a str,
    home: Option<&str>,
) -> Cow<'a, str> {
    let mut text = Cow::Borrowed(text);
    let apply =
        |text: &mut Cow<'a, str>, pattern: &Regex, f: &mut dyn FnMut(&Captures) -> String| {
            if pattern.is_match(text) {
                *text = Cow::Owned(
                    pattern
                        .replace_all(text, |caps: &Captures| f(caps))
                        .into_owned(),
                );
            }
        };
    apply(&mut text, &URL, &mut |caps| {
        // User info before an `@` goes with the path.
        let host = caps[2].rsplit('@').next().unwrap_or(&caps[2]);
        let port = caps.get(3).map_or("", |port| port.as_str());
        format!("{}://{}{port}", &caps[1], aliases.host(host))
    });
    apply(&mut text, &EMAIL, &mut |_| "<email>".to_owned());
    apply(&mut text, &SECRET_FIELD, &mut |caps| {
        format!("{}<secret>", &caps[1])
    });
    let servers: Vec<(String, String)> = aliases
        .server_hosts()
        .map(|(host, alias)| (host.to_owned(), alias.to_owned()))
        .collect();
    for (host, alias) in servers {
        if contains_ignoring_case(&text, &host) {
            text = Cow::Owned(replace_ignoring_case(&text, &host, &alias));
        }
    }
    if let Some(home) = home.filter(|home| home.len() > 1)
        && contains_ignoring_case(&text, home)
    {
        text = Cow::Owned(replace_ignoring_case(&text, home, "~"));
    }
    apply(&mut text, &IPV6, &mut |caps| {
        let candidate = &caps[2];
        let (address, zone) = match candidate.split_once('%') {
            Some((address, zone)) => (address, Some(zone)),
            None => (candidate, None),
        };
        match address.parse::<IpAddr>() {
            Ok(ip) if !ip.is_unspecified() && candidate.len() > 2 => format!(
                "{}{}{}{}",
                &caps[1],
                aliases.address(ip),
                zone.map(|zone| format!("%{zone}")).unwrap_or_default(),
                &caps[3]
            ),
            _ => caps[0].to_owned(),
        }
    });
    apply(
        &mut text,
        &IPV4,
        &mut |caps| match caps[1].parse::<IpAddr>() {
            Ok(ip) => aliases.address(ip),
            Err(_) => caps[0].to_owned(),
        },
    );
    apply(&mut text, &DOMAIN, &mut |caps| {
        let last = &caps[1];
        let host =
            (last.len() == 2 || TOP_LEVEL.contains(&last)) && !FILE_EXTENSIONS.contains(&last);
        if host {
            aliases.host(&caps[0])
        } else {
            caps[0].to_owned()
        }
    });
    apply(&mut text, &TOKEN, &mut |caps| {
        let token = &caps[2];
        if caps.get(1).is_some() || !looks_secret(token) {
            caps[0].to_owned()
        } else {
            "<secret>".to_owned()
        }
    });
    text
}

/// Random-looking: hex of 32 or more, or a mix with digits and no long
/// run of lowercase letters (which words and type names have).
fn looks_secret(token: &str) -> bool {
    let trimmed = token.trim_end_matches('=');
    if trimmed.len() >= 32 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    let digits = trimmed.chars().filter(char::is_ascii_digit).count();
    let mut run = 0;
    let mut longest = 0;
    for c in trimmed.chars() {
        run = if c.is_ascii_lowercase() { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    digits >= 2 && longest < 8
}

fn contains_ignoring_case(text: &str, needle: &str) -> bool {
    text.to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

/// Replaces ASCII-case-insensitive matches of `needle`.
fn replace_ignoring_case(text: &str, needle: &str, with: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let needle = needle.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find(&needle) {
        out.push_str(&text[at..at + found]);
        out.push_str(with);
        at += found + needle.len();
    }
    out.push_str(&text[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::super::alias::Aliases;
    use super::*;

    fn scrub(aliases: &mut Aliases, text: &str) -> String {
        line_with(aliases, text, Some("/home/amin")).into_owned()
    }

    #[test]
    fn filter_scrubs_third_party_url_with_token() {
        let mut aliases = Aliases::default();
        let out = scrub(
            &mut aliases,
            "error sending request for url (https://chat.example.org:8443/api/v1/x?token=abcd)",
        );
        assert_eq!(out, "error sending request for url (https://host-1:8443)");
    }

    #[test]
    fn registered_server_replaced_everywhere() {
        let mut aliases = Aliases::default();
        aliases.server("opencord2.mamad.xyz:443");
        assert_eq!(
            scrub(
                &mut aliases,
                "connecting to opencord2.mamad.xyz:443 (wss://opencord2.mamad.xyz/ws)"
            ),
            "connecting to server-A:443 (wss://server-A)"
        );
    }

    #[test]
    fn addresses_keep_kind_and_port() {
        let mut aliases = Aliases::default();
        assert_eq!(
            scrub(
                &mut aliases,
                "udp 192.168.1.5:7712 to 8.8.8.8, also [fd00::1]:443 and fe80::1%eth0"
            ),
            "udp lan-ip-1:7712 to public-ip-1, also [ipv6-lan-1]:443 and ipv6-lan-2%eth0"
        );
    }

    #[test]
    fn rust_paths_and_times_are_not_addresses() {
        let mut aliases = Aliases::default();
        let text = "INFO  media::video::screen  at 14:32:07 cafe::beef driver 32.0.15.6094 v0.0.1-canary.2";
        assert_eq!(scrub(&mut aliases, text), text);
    }

    #[test]
    fn home_folder_becomes_tilde() {
        let mut aliases = Aliases::default();
        assert_eq!(
            scrub(&mut aliases, "opened /home/amin/.local/share/opencord/logs"),
            "opened ~/.local/share/opencord/logs"
        );
        assert_eq!(
            line_with(
                &mut aliases,
                r"at C:\Users\Amin\AppData\Local",
                Some(r"C:\Users\Amin")
            ),
            r"at ~\AppData\Local"
        );
    }

    #[test]
    fn code_names_are_not_domains() {
        let mut aliases = Aliases::default();
        let text = "CoreError.notInitialized() at RustRepository.startScreenShare, \
                    provider.read and state.value, Opencord.app";
        assert_eq!(scrub(&mut aliases, text), text);
    }

    #[test]
    fn real_hosts_are_still_aliased() {
        let mut aliases = Aliases::default();
        assert_eq!(
            scrub(
                &mut aliases,
                "lookup quiet-river-1234.trycloudflare.com, amin-pc.local and lavalink.example.dev"
            ),
            "lookup host-1, host-2 and host-3"
        );
    }

    #[test]
    fn file_names_survive() {
        let mut aliases = Aliases::default();
        let text =
            "crash in ntdll.dll from windows.rs, app_start.dart, opencord.exe, code 0xC0000374";
        assert_eq!(scrub(&mut aliases, text), text);
    }

    #[test]
    fn bare_domains_and_emails_are_aliased() {
        let mut aliases = Aliases::default();
        assert_eq!(
            scrub(
                &mut aliases,
                "resolving api.example.com for someone@mail.example.net"
            ),
            "resolving host-1 for <email>"
        );
    }

    #[test]
    fn long_tokens_removed() {
        let mut aliases = Aliases::default();
        assert_eq!(
            scrub(&mut aliases, "token Zm9vYmFyYmF6cXV4MTIzNDU2Nzg5MA== sent"),
            "token <secret> sent"
        );
        assert_eq!(
            scrub(
                &mut aliases,
                "key 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
            ),
            "key <secret>"
        );
        let kept = "hash=9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08 Win32_System_Diagnostics_Debug";
        assert_eq!(scrub(&mut aliases, kept), kept);
    }

    #[test]
    fn corpus_has_no_leaks() {
        // A Windows checkout may give it CRLF line ends.
        let corpus = include_str!("testdata/leaks.txt").replace("\r\n", "\n");
        let (secrets, lines) = corpus.split_once("\n---\n").unwrap();
        let mut aliases = Aliases::default();
        aliases.server("opencord2.mamad.xyz:443");
        for line in lines.lines() {
            let out = scrub(&mut aliases, line);
            for secret in secrets.lines().filter(|secret| !secret.is_empty()) {
                assert!(
                    !out.to_lowercase().contains(&secret.to_lowercase()),
                    "{secret:?} leaked in {out:?}"
                );
            }
        }
    }
}
