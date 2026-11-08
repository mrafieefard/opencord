//! Signing (spec §9.2, §9.3): who wrote a plugin, and who may replace it.
//!
//! `SIGNATURE` signs a listing of every other file's path and SHA-256, so
//! a file added, removed or changed after signing breaks it. The listing
//! leaves out `KEY-ROTATION` too: that file carries its own signature, by
//! the key it replaces.
//!
//! A signature says who wrote a plugin, not which plugin may replace an
//! installed one: [`update_decision`] says that, from the key the server
//! pinned at the first signed install.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use data_encoding::{BASE64URL_NOPAD, HEXLOWER};
use ed25519_dalek::{Signature, Signer, VerifyingKey};
use sha2::{Digest, Sha256};

pub use ed25519_dalek::SigningKey;

use crate::manifest::Manifest;
use crate::package::{Package, PackageError};

/// The files signing writes, which the listing leaves out.
const SIGNATURE: &str = "SIGNATURE";
const KEY_ROTATION: &str = "KEY-ROTATION";

/// What a package's signature signs, before the listing's SHA-256.
const SIGNED_PREFIX: &[u8; 18] = b"opencord-plugin-v1";
/// What a key rotation signs, before the old and the new key.
const ROTATE_PREFIX: &[u8; 25] = b"opencord-plugin-rotate-v1";

const SIGNATURE_HEADER: &str = "opencord-plugin-signature-v1\n";
const ROTATION_HEADER: &str = "opencord-plugin-rotation-v1";
const KEY_FILE_HEADER: &str = "opencord-plugin-key-v1\n";

const KEY_PREFIX: &str = "ed25519:";

const NOT_THE_SAME_AUTHOR: &str = "this isn't from the same author";
const DEV_OVER_SIGNED: &str =
    "a signed plugin with this id is installed; dev mode can't replace it";

/// Why signing or checking a signature failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SigningError {
    /// `sign` was given a key the manifest's `public_key` doesn't name.
    KeyNotInManifest,
    SignatureWithoutKey,
    KeyWithoutSignature,
    /// The signature is malformed, or doesn't sign these files with the
    /// manifest's key.
    Mismatch,
    BadKey,
    RotationForm,
    RotationNotSigned,
    /// Writing `SIGNATURE` or `KEY-ROTATION` broke the package's limits.
    Package(PackageError),
    KeyFileUnreadable(String),
    NotAKeyFile,
}

impl fmt::Display for SigningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SigningError::KeyNotInManifest => {
                f.write_str("the manifest's public_key isn't this signing key")
            }
            SigningError::SignatureWithoutKey => {
                f.write_str("the plugin has a SIGNATURE but its manifest names no public_key")
            }
            SigningError::KeyWithoutSignature => {
                f.write_str("the manifest names a public_key but the plugin isn't signed")
            }
            SigningError::Mismatch => f.write_str(
                "the signature doesn't match: a file was changed, added or removed after signing",
            ),
            SigningError::BadKey => f.write_str("this isn't an ed25519 public key"),
            SigningError::RotationForm => {
                f.write_str("KEY-ROTATION isn't in the form opencord-plugin writes")
            }
            SigningError::RotationNotSigned => {
                f.write_str("KEY-ROTATION isn't signed by the key it replaces")
            }
            SigningError::Package(error) => error.fmt(f),
            SigningError::KeyFileUnreadable(error) => {
                write!(f, "the key file can't be read: {error}")
            }
            SigningError::NotAKeyFile => f.write_str("this isn't an opencord-plugin key file"),
        }
    }
}

impl std::error::Error for SigningError {}

impl From<PackageError> for SigningError {
    fn from(error: PackageError) -> SigningError {
        SigningError::Package(error)
    }
}

/// An Ed25519 public key, written `ed25519:` and its 32 bytes in base64url
/// without padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PublicKey(pub [u8; 32]);

impl PublicKey {
    /// Reads a key in the form [`PublicKey`]'s `Display` writes, and only a
    /// point on the curve.
    pub fn parse(text: &str) -> Result<PublicKey, SigningError> {
        let encoded = text.strip_prefix(KEY_PREFIX).ok_or(SigningError::BadKey)?;
        // Strict: no padding, and no bits past the 32 bytes, so each key
        // has one spelling and comparing them as text is comparing keys.
        let bytes = BASE64URL_NOPAD
            .decode(encoded.as_bytes())
            .map_err(|_| SigningError::BadKey)?;
        let key = PublicKey(bytes.try_into().map_err(|_| SigningError::BadKey)?);
        key.verifying()?;
        Ok(key)
    }

    /// The first 16 bytes of the key's SHA-256, as lowercase hex in eight
    /// groups of four, for people to compare.
    pub fn fingerprint(&self) -> String {
        let hash = Sha256::digest(self.0);
        hash[..16]
            .chunks(2)
            .map(|pair| HEXLOWER.encode(pair))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn of(key: &SigningKey) -> PublicKey {
        PublicKey(key.verifying_key().to_bytes())
    }

    fn verifying(&self) -> Result<VerifyingKey, SigningError> {
        VerifyingKey::from_bytes(&self.0).map_err(|_| SigningError::BadKey)
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{KEY_PREFIX}{}", BASE64URL_NOPAD.encode(&self.0))
    }
}

/// What a signature covers: for every file but `SIGNATURE` and
/// `KEY-ROTATION`, in byte order of their paths, the path, a newline, its
/// SHA-256 in lowercase hex and a newline. Paths can't hold a newline, so
/// the listing names exactly which files there are.
///
/// Hashes every file: the server runs it off its async threads.
pub fn listing(package: &Package) -> Vec<u8> {
    let mut files: Vec<(&str, &[u8])> = package
        .paths()
        .filter(|path| *path != SIGNATURE && *path != KEY_ROTATION)
        .filter_map(|path| package.get(path).map(|bytes| (path, bytes)))
        .collect();
    // `str`'s order is byte order. `paths` is already in it, but the format
    // depends on it.
    files.sort_unstable_by_key(|(path, _)| *path);
    let mut out = Vec::new();
    for (path, bytes) in files {
        out.extend_from_slice(path.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(HEXLOWER.encode(&Sha256::digest(bytes)).as_bytes());
        out.push(b'\n');
    }
    out
}

/// The 50 bytes a package's signature signs.
fn signed_message(package: &Package) -> Vec<u8> {
    let mut message = SIGNED_PREFIX.to_vec();
    message.extend_from_slice(&Sha256::digest(listing(package)));
    message
}

/// The 89 bytes a key rotation signs.
fn rotation_message(old: &PublicKey, new: &PublicKey) -> Vec<u8> {
    let mut message = ROTATE_PREFIX.to_vec();
    message.extend_from_slice(&old.0);
    message.extend_from_slice(&new.0);
    message
}

fn encode_signature(signature: &Signature) -> String {
    BASE64URL_NOPAD.encode(&signature.to_bytes())
}

fn decode_signature(text: &str) -> Option<Signature> {
    let bytes = BASE64URL_NOPAD.decode(text.as_bytes()).ok()?;
    Some(Signature::from_bytes(&bytes.try_into().ok()?))
}

/// Writes `SIGNATURE`, replacing one already there. `manifest` is the one
/// evaluated from the package's `manifest.lua`, and must name `key` as its
/// `public_key`: the listing covers `manifest.lua`, so the signature covers
/// the key it's checked with.
pub fn sign(
    package: &mut Package,
    manifest: &Manifest,
    key: &SigningKey,
) -> Result<(), SigningError> {
    let ours = PublicKey::of(key).to_string();
    if manifest.public_key.as_deref() != Some(ours.as_str()) {
        return Err(SigningError::KeyNotInManifest);
    }
    let signature = key.sign(&signed_message(package));
    let content = format!("{SIGNATURE_HEADER}{}\n", encode_signature(&signature));
    package.insert(SIGNATURE, content.into_bytes())?;
    Ok(())
}

/// Whether a package is signed, and by whom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureState {
    Unsigned,
    Signed(PublicKey),
}

/// Checks `SIGNATURE` against the key `manifest` names, where `manifest` is
/// the one evaluated from this package's `manifest.lua`. A package names a
/// key and is signed by it, or does neither.
///
/// Hashes every file: the server runs it off its async threads.
pub fn verify(package: &Package, manifest: &Manifest) -> Result<SignatureState, SigningError> {
    let (content, key) = match (package.get(SIGNATURE), manifest.public_key.as_deref()) {
        (None, None) => return Ok(SignatureState::Unsigned),
        (Some(_), None) => return Err(SigningError::SignatureWithoutKey),
        (None, Some(_)) => return Err(SigningError::KeyWithoutSignature),
        (Some(content), Some(key)) => (content, key),
    };
    let key = PublicKey::parse(key)?;
    let signature = std::str::from_utf8(content)
        .ok()
        .and_then(|text| text.strip_prefix(SIGNATURE_HEADER))
        .and_then(|text| text.strip_suffix('\n'))
        .and_then(decode_signature)
        .ok_or(SigningError::Mismatch)?;
    key.verifying()?
        .verify_strict(&signed_message(package), &signature)
        .map_err(|_| SigningError::Mismatch)?;
    Ok(SignatureState::Signed(key))
}

/// A statement, signed by `old`, that the plugin's key is now `new`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rotation {
    pub old: PublicKey,
    pub new: PublicKey,
}

/// The package's `KEY-ROTATION`, if it has one, once its signature by the
/// old key is checked. Whether it moves a pin is [`update_decision`]'s to
/// say.
pub fn rotation(package: &Package) -> Result<Option<Rotation>, SigningError> {
    let Some(content) = package.get(KEY_ROTATION) else {
        return Ok(None);
    };
    let (rotation, signature) = read_rotation(content).ok_or(SigningError::RotationForm)?;
    rotation
        .old
        .verifying()?
        .verify_strict(&rotation_message(&rotation.old, &rotation.new), &signature)
        .map_err(|_| SigningError::RotationNotSigned)?;
    Ok(Some(rotation))
}

/// Reads `KEY-ROTATION`'s four lines, exactly as [`write_rotation`] writes
/// them.
fn read_rotation(content: &[u8]) -> Option<(Rotation, Signature)> {
    let text = std::str::from_utf8(content).ok()?.strip_suffix('\n')?;
    let lines: Vec<&str> = text.split('\n').collect();
    let [header, old, new, signature] = lines.as_slice() else {
        return None;
    };
    if *header != ROTATION_HEADER {
        return None;
    }
    let key = |line: &str, label: &str| {
        line.strip_prefix(label)
            .and_then(|text| PublicKey::parse(text).ok())
    };
    let rotation = Rotation {
        old: key(old, "old ")?,
        new: key(new, "new ")?,
    };
    let signature = decode_signature(signature.strip_prefix("sig ")?)?;
    Some((rotation, signature))
}

/// Writes `KEY-ROTATION`: `old` hands the plugin over to `new`. The package
/// is then signed by `new`.
pub fn write_rotation(
    package: &mut Package,
    old: &SigningKey,
    new: &PublicKey,
) -> Result<(), SigningError> {
    let old_key = PublicKey::of(old);
    let signature = old.sign(&rotation_message(&old_key, new));
    let content = format!(
        "{ROTATION_HEADER}\nold {old_key}\nnew {new}\nsig {}\n",
        encode_signature(&signature)
    );
    package.insert(KEY_ROTATION, content.into_bytes())?;
    Ok(())
}

/// The key an installed plugin is pinned to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pin {
    None,
    Key(PublicKey),
}

/// How the version replacing it came.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incoming {
    Unsigned,
    Signed(PublicKey),
    /// A dev push, which is never signed.
    Dev,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateDecision {
    /// Allowed, and the plugin is pinned to `pin` from now on.
    Allow {
        pin: Pin,
    },
    Refuse(&'static str),
}

/// Whether `incoming` may replace a plugin pinned to `pin` (spec §9.2).
/// `rotation` is the incoming package's checked `KEY-ROTATION`; it counts
/// only from the pinned key, and only for a package its new key signed.
pub fn update_decision(
    pin: &Pin,
    incoming: &Incoming,
    rotation: Option<&Rotation>,
) -> UpdateDecision {
    match (*pin, *incoming) {
        (Pin::None, Incoming::Unsigned | Incoming::Dev) => UpdateDecision::Allow { pin: Pin::None },
        (Pin::None, Incoming::Signed(key)) => UpdateDecision::Allow { pin: Pin::Key(key) },
        (Pin::Key(_), Incoming::Dev) => UpdateDecision::Refuse(DEV_OVER_SIGNED),
        (Pin::Key(pinned), Incoming::Signed(key)) if key == pinned => {
            UpdateDecision::Allow { pin: Pin::Key(key) }
        }
        (Pin::Key(pinned), Incoming::Signed(key))
            if rotation.is_some_and(|rotation| rotation.old == pinned && rotation.new == key) =>
        {
            UpdateDecision::Allow { pin: Pin::Key(key) }
        }
        (Pin::Key(_), Incoming::Signed(_) | Incoming::Unsigned) => {
            UpdateDecision::Refuse(NOT_THE_SAME_AUTHOR)
        }
    }
}

/// A new signing key, from the operating system's random source.
pub fn generate() -> SigningKey {
    let mut secret = [0; 32];
    // The OS random source failing leaves nothing secure to fall back to.
    getrandom::fill(&mut secret).expect("the operating system's random source is unavailable");
    SigningKey::from_bytes(&secret)
}

/// Writes a key file that only its owner can read (on Unix), refusing to
/// replace one already there: it may be the only copy of a key.
pub fn write_key(path: &Path, key: &SigningKey) -> io::Result<()> {
    let content = format!(
        "{KEY_FILE_HEADER}{}\n",
        BASE64URL_NOPAD.encode(key.as_bytes())
    );
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()
}

/// Reads a key file [`write_key`] wrote.
pub fn read_key(path: &Path) -> Result<SigningKey, SigningError> {
    let content =
        fs::read(path).map_err(|error| SigningError::KeyFileUnreadable(error.to_string()))?;
    let secret: [u8; 32] = content
        .strip_prefix(KEY_FILE_HEADER.as_bytes())
        .and_then(|rest| rest.strip_suffix(b"\n"))
        .and_then(|encoded| BASE64URL_NOPAD.decode(encoded).ok())
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(SigningError::NotAKeyFile)?;
    Ok(SigningKey::from_bytes(&secret))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest;

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn public(key: &SigningKey) -> PublicKey {
        PublicKey(key.verifying_key().to_bytes())
    }

    /// A plugin's package and its manifest, which names `key` when given.
    fn plugin(key: Option<&SigningKey>) -> (Package, Manifest) {
        let line = key
            .map(|key| format!("m.public_key = \"{}\"", public(key)))
            .unwrap_or_default();
        let source = format!(
            r#"local m = {{
  id = "tickets", name = "Ticket System", version = "1.2.0", author = "Amin",
  license = "MIT", api = 1, server = {{ "server/main.luau" }},
}}
{line}
return m
"#
        );
        let manifest = manifest::evaluate(&source).unwrap();
        let mut package = Package::empty();
        package.insert("manifest.lua", source.into_bytes()).unwrap();
        package
            .insert("server/main.luau", b"print('hi')".to_vec())
            .unwrap();
        package
            .insert("assets/icon.png", vec![0x89, b'P', b'N', b'G'])
            .unwrap();
        (package, manifest)
    }

    fn signed(key: &SigningKey) -> (Package, Manifest) {
        let (mut package, manifest) = plugin(Some(key));
        sign(&mut package, &manifest, key).unwrap();
        (package, manifest)
    }

    /// 32 bytes that aren't an Ed25519 public key.
    fn not_a_point() -> [u8; 32] {
        (0..=u8::MAX)
            .map(|byte| [byte; 32])
            .find(|bytes| ed25519_dalek::VerifyingKey::from_bytes(bytes).is_err())
            .unwrap()
    }

    fn refused(package: &Package, manifest: &Manifest) -> String {
        verify(package, manifest).unwrap_err().to_string()
    }

    const MISMATCH: &str =
        "the signature doesn't match: a file was changed, added or removed after signing";
    const NOT_THE_AUTHOR: UpdateDecision =
        UpdateDecision::Refuse("this isn't from the same author");

    #[test]
    fn a_signed_package_verifies() {
        let author = key(1);
        let (package, manifest) = signed(&author);
        assert_eq!(
            verify(&package, &manifest),
            Ok(SignatureState::Signed(public(&author)))
        );

        // SIGNATURE's form: its header, 86 base64url characters, a newline.
        let content = std::str::from_utf8(package.get("SIGNATURE").unwrap()).unwrap();
        let encoded = content
            .strip_prefix("opencord-plugin-signature-v1\n")
            .and_then(|rest| rest.strip_suffix('\n'))
            .unwrap();
        assert_eq!(encoded.len(), 86);

        // It signs exactly "opencord-plugin-v1" and the listing's SHA-256.
        // Catches: signing the listing itself, or another prefix, which
        // would verify here but not in another build of the tool.
        let mut message = b"opencord-plugin-v1".to_vec();
        message.extend_from_slice(&Sha256::digest(listing(&package)));
        assert_eq!(message.len(), 50);
        let expected = author.sign(&message).to_bytes();
        assert_eq!(
            data_encoding::BASE64URL_NOPAD
                .decode(encoded.as_bytes())
                .unwrap(),
            expected
        );

        // Through a zip and back, it still verifies.
        let again = Package::read_zip(&package.write_zip()).unwrap();
        assert_eq!(
            verify(&again, &manifest),
            Ok(SignatureState::Signed(public(&author)))
        );

        // Nothing named and nothing signed is unsigned.
        let (package, manifest) = plugin(None);
        assert_eq!(verify(&package, &manifest), Ok(SignatureState::Unsigned));
    }

    // Catches: a listing of paths without their content, or one that skips
    // scripts or the manifest.
    #[test]
    fn a_changed_file_fails() {
        let (signed_package, manifest) = signed(&key(1));
        for path in ["manifest.lua", "server/main.luau", "assets/icon.png"] {
            let mut package = signed_package.clone();
            let mut bytes = package.get(path).unwrap().to_vec();
            bytes.push(b' ');
            package.insert(path, bytes).unwrap();
            assert_eq!(refused(&package, &manifest), MISMATCH, "{path}");
        }

        // A changed signature is a bad one.
        let mut package = signed_package.clone();
        let mut content = package.get("SIGNATURE").unwrap().to_vec();
        let flipped = content.len() - 10;
        content[flipped] = if content[flipped] == b'A' { b'B' } else { b'A' };
        package.insert("SIGNATURE", content).unwrap();
        assert_eq!(refused(&package, &manifest), MISMATCH);

        // So is one that isn't in SIGNATURE's form.
        for content in [&b"opencord-plugin-signature-v1\n"[..], b"", b"\xff"] {
            let mut package = signed_package.clone();
            package.insert("SIGNATURE", content.to_vec()).unwrap();
            assert_eq!(refused(&package, &manifest), MISMATCH);
        }
    }

    // Catches: a listing of the files the manifest names, which leaves
    // anything else unchecked.
    #[test]
    fn an_added_file_fails() {
        let (mut package, manifest) = signed(&key(1));
        package
            .insert("server/extra.lua", b"os.exit()".to_vec())
            .unwrap();
        assert_eq!(refused(&package, &manifest), MISMATCH);

        let (mut package, manifest) = signed(&key(1));
        package.insert("assets/x.png", Vec::new()).unwrap();
        assert_eq!(refused(&package, &manifest), MISMATCH);
    }

    // Catches: checking only the files still there against the listing.
    #[test]
    fn a_removed_file_fails() {
        let (package, manifest) = signed(&key(1));
        let mut fewer = Package::empty();
        for path in package.paths().filter(|path| *path != "assets/icon.png") {
            fewer
                .insert(path, package.get(path).unwrap().to_vec())
                .unwrap();
        }
        assert_eq!(fewer.paths().count(), package.paths().count() - 1);
        assert_eq!(refused(&fewer, &manifest), MISMATCH);
    }

    // Catches: checking the signature against any key, or the signer's own
    // key rather than the one the manifest names.
    #[test]
    fn a_wrong_key_fails() {
        let author = key(1);
        let other = key(2);
        let (mut package, manifest) = signed(&author);
        let mut message = b"opencord-plugin-v1".to_vec();
        message.extend_from_slice(&Sha256::digest(listing(&package)));
        let forged = format!(
            "opencord-plugin-signature-v1\n{}\n",
            data_encoding::BASE64URL_NOPAD.encode(&other.sign(&message).to_bytes())
        );
        package.insert("SIGNATURE", forged.into_bytes()).unwrap();
        assert_eq!(refused(&package, &manifest), MISMATCH);

        // sign refuses a key the manifest doesn't name.
        let (mut package, manifest) = plugin(Some(&author));
        assert_eq!(
            sign(&mut package, &manifest, &other),
            Err(SigningError::KeyNotInManifest)
        );
        let (mut package, manifest) = plugin(None);
        assert_eq!(
            sign(&mut package, &manifest, &author),
            Err(SigningError::KeyNotInManifest)
        );
        assert!(package.get("SIGNATURE").is_none());

        // A key that isn't a point on the curve can't verify anything.
        let (package, mut manifest) = signed(&author);
        manifest.public_key = Some(PublicKey(not_a_point()).to_string());
        assert_eq!(
            refused(&package, &manifest),
            "this isn't an ed25519 public key"
        );
    }

    // Catches: treating a package as unsigned when its manifest names no
    // key, which lets anyone take a signed plugin's key out of it.
    #[test]
    fn signature_without_key_is_refused() {
        let (signed_package, _) = signed(&key(1));
        let (mut package, manifest) = plugin(None);
        package
            .insert(
                "SIGNATURE",
                signed_package.get("SIGNATURE").unwrap().to_vec(),
            )
            .unwrap();
        assert_eq!(
            refused(&package, &manifest),
            "the plugin has a SIGNATURE but its manifest names no public_key"
        );
    }

    // Catches: treating a package whose SIGNATURE was deleted as unsigned.
    #[test]
    fn key_without_signature_is_refused() {
        let (package, manifest) = plugin(Some(&key(1)));
        assert_eq!(
            refused(&package, &manifest),
            "the manifest names a public_key but the plugin isn't signed"
        );
    }

    // Catches: listing files in the order they were added, or sorting
    // them ignoring case, either of which another build can't repeat.
    #[test]
    fn listing_is_canonical() {
        let mut one = Package::empty();
        one.insert("server/a.lua", Vec::new()).unwrap();
        one.insert("server/B.lua", b"return {}".to_vec()).unwrap();
        one.insert("SIGNATURE", b"left out".to_vec()).unwrap();
        one.insert("KEY-ROTATION", b"left out".to_vec()).unwrap();
        let mut two = Package::empty();
        two.insert("KEY-ROTATION", b"also left out".to_vec())
            .unwrap();
        two.insert("server/B.lua", b"return {}".to_vec()).unwrap();
        two.insert("server/a.lua", Vec::new()).unwrap();
        assert_eq!(listing(&one), listing(&two));
        assert_eq!(
            String::from_utf8(listing(&one)).unwrap(),
            "server/B.lua\n\
             7df8f0cd9e1cd0b204a760783671c0ccca4a3258dc7c51f59e9e2295c6c25315\n\
             server/a.lua\n\
             e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n"
        );
    }

    #[test]
    fn public_keys_parse_and_print() {
        let author = key(1);
        let text = public(&author).to_string();
        assert!(text.starts_with("ed25519:"));
        assert_eq!(text.len(), "ed25519:".len() + 43);
        assert_eq!(PublicKey::parse(&text), Ok(public(&author)));
        // The manifest accepts the form the key prints in.
        let (_, manifest) = plugin(Some(&author));
        assert_eq!(manifest.public_key.as_deref(), Some(text.as_str()));

        let bad = |text: &str| PublicKey::parse(text).unwrap_err().to_string();
        let message = "this isn't an ed25519 public key";
        assert_eq!(bad(&text[1..]), message);
        assert_eq!(bad(&format!("{text}A")), message);
        assert_eq!(bad(&format!("{text}=")), message);
        assert_eq!(bad(&text.replace("ed25519:", "ED25519:")), message);
        assert_eq!(bad(""), message);
        assert_eq!(bad(&PublicKey(not_a_point()).to_string()), message);

        // The first 16 bytes of SHA-256 of 32 zero bytes, in groups of four.
        assert_eq!(
            PublicKey([0; 32]).fingerprint(),
            "6668 7aad f862 bd77 6c8f c18b 8e9f 8e20"
        );
    }

    // Catches: a pin that never moves, a pin taken from an unsigned update,
    // or dev mode replacing a signed plugin.
    #[test]
    fn update_decision_table() {
        let a = public(&key(1));
        let b = public(&key(2));
        let rotation = Rotation { old: a, new: b };
        let allow = |pin| UpdateDecision::Allow { pin };
        let cases = [
            // §9.2's rows.
            (Pin::None, Incoming::Unsigned, None, allow(Pin::None)),
            (Pin::None, Incoming::Signed(a), None, allow(Pin::Key(a))),
            (Pin::Key(a), Incoming::Signed(a), None, allow(Pin::Key(a))),
            (
                Pin::Key(a),
                Incoming::Signed(b),
                Some(&rotation),
                allow(Pin::Key(b)),
            ),
            (Pin::Key(a), Incoming::Signed(b), None, NOT_THE_AUTHOR),
            (Pin::Key(a), Incoming::Unsigned, None, NOT_THE_AUTHOR),
            // Dev pushes.
            (Pin::None, Incoming::Dev, None, allow(Pin::None)),
            (
                Pin::Key(a),
                Incoming::Dev,
                None,
                UpdateDecision::Refuse(
                    "a signed plugin with this id is installed; dev mode can't replace it",
                ),
            ),
        ];
        for (pin, incoming, rotation, expected) in cases {
            assert_eq!(
                update_decision(&pin, &incoming, rotation),
                expected,
                "{pin:?} {incoming:?} {rotation:?}"
            );
        }
    }

    // Catches: a rotation that counts whoever signed the package, or
    // whatever key it starts from.
    #[test]
    fn rotation_moves_the_pin() {
        let old = key(1);
        let new = key(2);
        let (mut package, manifest) = plugin(Some(&new));
        assert_eq!(rotation(&package), Ok(None));
        write_rotation(&mut package, &old, &public(&new)).unwrap();
        sign(&mut package, &manifest, &new).unwrap();

        let content = String::from_utf8(package.get("KEY-ROTATION").unwrap().to_vec()).unwrap();
        let lines: Vec<&str> = content.split('\n').collect();
        assert_eq!(lines.len(), 5, "{content:?}");
        assert_eq!(lines[0], "opencord-plugin-rotation-v1");
        assert_eq!(lines[1], format!("old {}", public(&old)));
        assert_eq!(lines[2], format!("new {}", public(&new)));
        assert!(lines[3].starts_with("sig "));
        assert_eq!(lines[3].len(), "sig ".len() + 86);
        assert_eq!(lines[4], "");

        let found = rotation(&package).unwrap().unwrap();
        assert_eq!(
            found,
            Rotation {
                old: public(&old),
                new: public(&new),
            }
        );
        let incoming = match verify(&package, &manifest).unwrap() {
            SignatureState::Signed(key) => Incoming::Signed(key),
            SignatureState::Unsigned => panic!("the package is signed"),
        };
        let pinned = Pin::Key(public(&old));
        assert_eq!(
            update_decision(&pinned, &incoming, Some(&found)),
            UpdateDecision::Allow {
                pin: Pin::Key(public(&new)),
            }
        );
        // Without the rotation, the new key is another author's.
        assert_eq!(update_decision(&pinned, &incoming, None), NOT_THE_AUTHOR);

        // The rotation counts only from the pinned key, and only for a
        // package signed by the key it names.
        let third = public(&key(3));
        assert_eq!(
            update_decision(&Pin::Key(third), &incoming, Some(&found)),
            NOT_THE_AUTHOR
        );
        for incoming in [
            Incoming::Signed(third),
            Incoming::Signed(public(&old)),
            Incoming::Unsigned,
            Incoming::Dev,
        ] {
            let expected = match incoming {
                // Still the pinned key: allowed, and the pin stays.
                Incoming::Signed(key) if key == public(&old) => UpdateDecision::Allow {
                    pin: Pin::Key(public(&old)),
                },
                Incoming::Dev => UpdateDecision::Refuse(
                    "a signed plugin with this id is installed; dev mode can't replace it",
                ),
                _ => NOT_THE_AUTHOR,
            };
            assert_eq!(
                update_decision(&pinned, &incoming, Some(&found)),
                expected,
                "{incoming:?}"
            );
        }
    }

    // Catches: reading KEY-ROTATION's keys without checking that the old
    // key signed them, which lets anyone move a plugin's pin to their key.
    #[test]
    fn a_rotation_signed_by_another_key_is_refused() {
        let old = public(&key(1));
        let thief = key(3);
        let mut statement = b"opencord-plugin-rotate-v1".to_vec();
        statement.extend_from_slice(&old.0);
        statement.extend_from_slice(&public(&thief).0);
        assert_eq!(statement.len(), 89);
        let forged = format!(
            "opencord-plugin-rotation-v1\nold {old}\nnew {}\nsig {}\n",
            public(&thief),
            data_encoding::BASE64URL_NOPAD.encode(&thief.sign(&statement).to_bytes())
        );
        let (mut package, _) = plugin(Some(&thief));
        package.insert("KEY-ROTATION", forged.into_bytes()).unwrap();
        let not_signed = "KEY-ROTATION isn't signed by the key it replaces";
        assert_eq!(rotation(&package).unwrap_err().to_string(), not_signed);

        // A real rotation whose new key was changed afterwards.
        let (mut package, _) = plugin(Some(&thief));
        write_rotation(&mut package, &key(1), &public(&key(2))).unwrap();
        let content = String::from_utf8(package.get("KEY-ROTATION").unwrap().to_vec())
            .unwrap()
            .replace(&public(&key(2)).to_string(), &public(&thief).to_string());
        package
            .insert("KEY-ROTATION", content.into_bytes())
            .unwrap();
        assert_eq!(rotation(&package).unwrap_err().to_string(), not_signed);

        // A rotation the thief signed from their own key is valid, but it
        // doesn't start from the pinned key.
        let (mut package, manifest) = plugin(Some(&key(4)));
        write_rotation(&mut package, &thief, &public(&key(4))).unwrap();
        sign(&mut package, &manifest, &key(4)).unwrap();
        let found = rotation(&package).unwrap().unwrap();
        assert_eq!(
            update_decision(
                &Pin::Key(old),
                &Incoming::Signed(public(&key(4))),
                Some(&found)
            ),
            NOT_THE_AUTHOR
        );

        // KEY-ROTATION in any other form is refused.
        let form = "KEY-ROTATION isn't in the form opencord-plugin writes";
        let (mut good, _) = plugin(None);
        write_rotation(&mut good, &key(1), &public(&key(2))).unwrap();
        let good = String::from_utf8(good.get("KEY-ROTATION").unwrap().to_vec()).unwrap();
        for content in [
            String::new(),
            good.trim_end().to_owned(),
            format!("{good}\n"),
            good.replace("rotation-v1", "rotation-v2"),
            good.replace("old ", "old  "),
            good.replacen("ed25519:", "ed25519:A", 1),
            good.replace("\n", "\r\n"),
        ] {
            let (mut package, _) = plugin(None);
            package
                .insert("KEY-ROTATION", content.clone().into_bytes())
                .unwrap();
            assert_eq!(
                rotation(&package).unwrap_err().to_string(),
                form,
                "{content:?}"
            );
        }
    }

    // Catches: a key file others can read, or one that overwrites the
    // only copy of a developer's key.
    #[test]
    fn key_files_round_trip_and_are_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plugin.key");
        let key = generate();
        assert_ne!(generate().to_bytes(), key.to_bytes());

        write_key(&path, &key).unwrap();
        assert_eq!(read_key(&path).unwrap().to_bytes(), key.to_bytes());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("opencord-plugin-key-v1\n"));
        assert!(content.ends_with('\n'));
        assert_eq!(content.len(), "opencord-plugin-key-v1\n".len() + 43 + 1);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "{mode:o}");
        }

        // Never overwritten.
        let error = write_key(&path, &generate()).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(read_key(&path).unwrap().to_bytes(), key.to_bytes());

        let other = dir.path().join("other.key");
        std::fs::write(&other, "opencord-plugin-key-v1\nAAAA\n").unwrap();
        assert_eq!(
            read_key(&other).unwrap_err().to_string(),
            "this isn't an opencord-plugin key file"
        );
        let missing = read_key(&dir.path().join("missing.key")).unwrap_err();
        assert!(
            missing
                .to_string()
                .starts_with("the key file can't be read: "),
            "{missing}"
        );
    }
}
