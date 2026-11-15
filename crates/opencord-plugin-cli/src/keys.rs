use std::path::{Path, PathBuf};

use opencord_plugins::signing::{self, PublicKey};

pub fn default_key_path() -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return Ok(PathBuf::from(appdata)
                .join("opencord-plugin")
                .join("signing.key"));
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME")
            && !xdg.is_empty()
        {
            return Ok(PathBuf::from(xdg)
                .join("opencord-plugin")
                .join("signing.key"));
        }
        if let Some(home) = std::env::var_os("HOME")
            && !home.is_empty()
        {
            return Ok(PathBuf::from(home)
                .join(".config")
                .join("opencord-plugin")
                .join("signing.key"));
        }
    }
    Err("cannot determine config directory (neither XDG_CONFIG_HOME nor HOME is set)".into())
}

pub fn keygen(out: Option<&Path>) -> Result<(String, String), String> {
    let key_path = match out {
        Some(p) => p.to_path_buf(),
        None => default_key_path()?,
    };

    if key_path.exists() {
        return Err(format!(
            "key file already exists at '{}'",
            key_path.display()
        ));
    }

    if let Some(parent) = key_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create directory '{}': {e}", parent.display()))?;
    }

    let key = signing::generate();
    signing::write_key(&key_path, &key)
        .map_err(|e| format!("failed to write key file '{}': {e}", key_path.display()))?;

    let pubkey = PublicKey::from(&key);
    let fingerprint = pubkey.fingerprint();

    println!("Public key: {pubkey}");
    println!("Fingerprint: {fingerprint}");
    Ok((pubkey.to_string(), fingerprint))
}
