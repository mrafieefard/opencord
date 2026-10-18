use opencord_common::upload::{
    effective_upload_limit, is_blocked, normalize_extension, sanitize_filename,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    limits: Vec<LimitCase>,
    filenames: Vec<FilenameCase>,
    extensions: Vec<ExtensionCase>,
    blocked: Vec<BlockedCase>,
}

#[derive(Deserialize)]
struct LimitCase {
    owner: bool,
    everyone: u64,
    roles: Vec<Option<u64>>,
    ceiling: u64,
    expected: u64,
}

#[derive(Deserialize)]
struct FilenameCase {
    input: String,
    expected: String,
}

#[derive(Deserialize)]
struct ExtensionCase {
    input: String,
    expected: Option<String>,
}

#[derive(Deserialize)]
struct BlockedCase {
    filename: String,
    list: Vec<String>,
    expected: bool,
}

#[test]
fn fixture_rules_match() {
    let fixture: Fixture = serde_json::from_str(include_str!("upload_rules.json")).unwrap();

    for case in fixture.limits {
        let actual =
            effective_upload_limit(case.owner, case.everyone, case.roles.clone(), case.ceiling);
        assert_eq!(actual, case.expected, "limit case failed");
    }

    for case in fixture.filenames {
        let actual = sanitize_filename(&case.input);
        assert_eq!(actual, case.expected, "filename {:?} failed", case.input);
    }

    for case in fixture.extensions {
        let actual = normalize_extension(&case.input);
        match case.expected {
            Some(expected) => {
                assert_eq!(
                    actual.unwrap(),
                    expected,
                    "extension {:?} failed",
                    case.input
                )
            }
            None => assert!(
                actual.is_err(),
                "extension {:?} should fail, got {:?}",
                case.input,
                actual
            ),
        }
    }

    for case in fixture.blocked {
        let actual = is_blocked(&case.filename, &case.list);
        assert_eq!(
            actual, case.expected,
            "blocked case {:?} failed",
            case.filename
        );
    }
}

#[test]
fn generated_long_filename_is_cut_to_200_chars_keeping_extension() {
    let input = format!("{}.png", "a".repeat(296));
    let sanitized = sanitize_filename(&input);
    assert_eq!(sanitized.chars().count(), 200);
    assert!(sanitized.ends_with(".png"));
}
