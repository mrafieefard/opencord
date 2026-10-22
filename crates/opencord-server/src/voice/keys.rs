//! The voice-signing key: an Ed25519 key, made on first start and kept in
//! the data directory. It signs voice tokens (checked by voice nodes, which
//! get its public half) and media tokens.

use std::fs;
use std::path::Path;

use ed25519_dalek::SigningKey;

use crate::random;
use crate::tls::write_private;

pub const VOICE_KEY_FILE: &str = "voice-signing.key";

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("could not read or write {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{path} is not a voice-signing key")]
    Invalid { path: String },
}

/// The key in `dir`, created there on first use.
pub fn load_or_generate(dir: &Path) -> Result<SigningKey, KeyError> {
    let path = dir.join(VOICE_KEY_FILE);
    let io_error = |source| KeyError::Io {
        path: path.display().to_string(),
        source,
    };
    if path.exists() {
        let bytes = fs::read(&path).map_err(io_error)?;
        let seed: [u8; 32] = bytes.try_into().map_err(|_| KeyError::Invalid {
            path: path.display().to_string(),
        })?;
        return Ok(SigningKey::from_bytes(&seed));
    }
    fs::create_dir_all(dir).map_err(io_error)?;
    let seed = random::bytes::<32>();
    write_private(&path, &seed).map_err(io_error)?;
    Ok(SigningKey::from_bytes(&seed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_a_key_once_and_reuses_it() {
        let dir = tempfile::tempdir().unwrap();

        let first = load_or_generate(dir.path()).unwrap();
        let second = load_or_generate(dir.path()).unwrap();

        assert_eq!(first.to_bytes(), second.to_bytes());
    }

    #[cfg(unix)]
    #[test]
    fn the_key_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();

        load_or_generate(dir.path()).unwrap();

        let mode = fs::metadata(dir.path().join(VOICE_KEY_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_damaged_key_file_is_an_error_not_a_new_key() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(VOICE_KEY_FILE), b"short").unwrap();

        assert!(matches!(
            load_or_generate(dir.path()),
            Err(KeyError::Invalid { .. })
        ));
    }
}
