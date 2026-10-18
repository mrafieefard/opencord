//! Shared upload rules, constants, filename sanitizing, and extension handling.

use std::ops::RangeInclusive;

use crate::validation::ValidationError;

pub const MIB: u64 = 1_048_576;
pub const PIECE_BYTES: u32 = 8_388_608;
pub const MAX_ATTACHMENTS: usize = 10;
pub const MAX_UNFINISHED_PER_USER: u32 = 10;
pub const EVERYONE_DEFAULT_BYTES: u64 = 52_428_800;
pub const MIN_ROLE_LIMIT_BYTES: u64 = MIB;
pub const MAX_FILE_MB_DEFAULT: u64 = 4096;
pub const MAX_FILE_MB_RANGE: RangeInclusive<u64> = 1..=102_400;
pub const VOICE_MAX_SECONDS_DEFAULT: u32 = 300;
pub const VOICE_MAX_SECONDS_RANGE: RangeInclusive<u32> = 30..=1_800;
pub const TEMP_HEADROOM_BYTES: u64 = 512 * MIB;
pub const POSTER_MAX_BYTES: usize = 2 * 1_048_576;
pub const IMAGE_PROCESS_MAX_BYTES: u64 = 100 * MIB;
pub const BLOCKED_MAX_ENTRIES: usize = 100;
pub const BLOCKED_ENTRY_MAX_CHARS: usize = 20;
pub const FILENAME_MAX_CHARS: usize = 200;
pub const VOICE_FILENAME: &str = "voice-message.ogg";

pub const BLOCK_PROGRAMS: [&str; 31] = [
    "exe", "msi", "msix", "appx", "bat", "cmd", "com", "scr", "pif", "cpl", "ps1", "psm1", "vbs",
    "vbe", "js", "jse", "wsf", "wsh", "hta", "lnk", "reg", "jar", "apk", "app", "dmg", "pkg",
    "deb", "rpm", "appimage", "sh", "run",
];

const RESERVED_STEMS: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Computes the effective upload limit for a member in a server.
pub fn effective_upload_limit(
    is_owner: bool,
    everyone: u64,
    roles: impl IntoIterator<Item = Option<u64>>,
    ceiling: u64,
) -> u64 {
    if is_owner {
        return ceiling;
    }
    let mut max_limit = everyone;
    for role in roles {
        if let Some(limit) = role {
            max_limit = max_limit.max(limit);
        }
    }
    max_limit.min(ceiling)
}

/// The maximum byte size allowed for a voice message of `max_seconds`.
pub fn voice_max_bytes(max_seconds: u32) -> u64 {
    max_seconds as u64 * 8_000 + 65_536
}

/// Sanitizes an uploaded filename according to the shared rules.
pub fn sanitize_filename(input: &str) -> String {
    // 1. Keep only the part after the last / or \
    let name = input.rsplit(|c| c == '/' || c == '\\').next().unwrap_or(input);

    // 2. Drop control characters
    let mut name: String = name.chars().filter(|c| !c.is_control()).collect();

    // 3. Trim trailing . and spaces, and leading spaces
    name = name.trim_start_matches(' ').to_string();
    name = name.trim_end_matches(|c| c == '.' || c == ' ').to_string();

    // 6. Empty -> "file"
    if name.is_empty() {
        return "file".to_string();
    }

    // 4. Reserved DOS device names
    let stem = match name.split_once('.') {
        Some((s, _)) => s,
        None => &name,
    };
    if RESERVED_STEMS.iter().any(|&r| stem.eq_ignore_ascii_case(r)) {
        name = format!("_{name}");
    }

    // 5. Length > 200 chars: cut the stem and keep the extension (<= 20 chars)
    if name.chars().count() > FILENAME_MAX_CHARS {
        let has_valid_ext = if let Some((stem, ext)) = name.rsplit_once('.') {
            let ext_chars = ext.chars().count();
            if ext_chars <= 20 {
                let stem_budget = FILENAME_MAX_CHARS.saturating_sub(ext_chars + 1);
                let truncated_stem: String = stem.chars().take(stem_budget).collect();
                Some(format!("{truncated_stem}.{ext}"))
            } else {
                None
            }
        } else {
            None
        };
        name = has_valid_ext.unwrap_or_else(|| name.chars().take(FILENAME_MAX_CHARS).collect());
    }

    if name.is_empty() {
        return "file".to_string();
    }

    name
}

/// Normalizes an extension: trim, strip one leading dot, lowercase.
pub fn normalize_extension(input: &str) -> Result<String, ValidationError> {
    let trimmed = input.trim();
    let s = trimmed.strip_prefix('.').unwrap_or(trimmed);
    let lower = s.to_lowercase();
    let char_count = lower.chars().count();
    if char_count == 0 {
        return Err(ValidationError::Empty { field: "extension" });
    }
    if char_count > 20 {
        return Err(ValidationError::TooLong {
            field: "extension",
            max: 20,
        });
    }
    if lower.starts_with('.') || lower.ends_with('.') || lower.contains("..") {
        return Err(ValidationError::Invalid { field: "extension" });
    }
    for c in lower.chars() {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '.' {
            return Err(ValidationError::Invalid { field: "extension" });
        }
    }
    Ok(lower)
}

/// Normalizes a list of blocked extensions: normalize each, drop duplicates, sort, at most 100.
pub fn normalize_blocked_list(entries: &[String]) -> Result<Vec<String>, ValidationError> {
    let mut list = Vec::with_capacity(entries.len());
    for entry in entries {
        list.push(normalize_extension(entry)?);
    }
    list.sort();
    list.dedup();
    if list.len() > BLOCKED_MAX_ENTRIES {
        return Err(ValidationError::TooLong {
            field: "blocked extensions",
            max: BLOCKED_MAX_ENTRIES,
        });
    }
    Ok(list)
}

/// Checks if a sanitized filename matches any blocked extension.
pub fn is_blocked(sanitized_filename: &str, blocked: &[String]) -> bool {
    let lower = sanitized_filename.to_lowercase();
    blocked.iter().any(|entry| {
        if lower.len() > entry.len() + 1 && lower.ends_with(entry) {
            lower.as_bytes()[lower.len() - entry.len() - 1] == b'.'
        } else {
            false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_max_bytes_for_300_seconds() {
        assert_eq!(voice_max_bytes(300), 2_465_536);
    }

    #[test]
    fn normalize_blocked_list_sorts_and_dedupes() {
        let entries = vec![
            "EXE".to_string(),
            ".exe".to_string(),
            "bat".to_string(),
        ];
        let normalized = normalize_blocked_list(&entries).unwrap();
        assert_eq!(normalized, vec!["bat".to_string(), "exe".to_string()]);
    }

    #[test]
    fn normalize_blocked_list_exceeding_max_fails() {
        let entries: Vec<String> = (0..101).map(|i| format!("e{i}")).collect();
        assert!(normalize_blocked_list(&entries).is_err());
    }
}
