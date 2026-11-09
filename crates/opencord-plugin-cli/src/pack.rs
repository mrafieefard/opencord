use std::fs;
use std::path::{Path, PathBuf};

use opencord_plugins::manifest;
use opencord_plugins::package::Package;
use opencord_plugins::signing::{self, PublicKey};

pub fn rewrite_manifest_public_key(manifest_path: &Path, pubkey: &str) -> Result<(), String> {
    let content = fs::read_to_string(manifest_path).map_err(|e| {
        format!(
            "failed to read manifest at '{}': {e}",
            manifest_path.display()
        )
    })?;

    let lines: Vec<&str> = content.lines().collect();
    let mut has_pk_line = false;
    for line in &lines {
        let trimmed = line.trim_start();
        if trimmed.starts_with("public_key =") || trimmed.starts_with("public_key=") {
            has_pk_line = true;
            break;
        }
    }

    let mut new_lines = Vec::new();
    if has_pk_line {
        for line in lines {
            let trimmed = line.trim_start();
            if trimmed.starts_with("public_key =") || trimmed.starts_with("public_key=") {
                let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                new_lines.push(format!("{indent}public_key = \"{pubkey}\","));
            } else {
                new_lines.push(line.to_string());
            }
        }
    } else {
        let mut inserted = false;
        for line in lines {
            new_lines.push(line.to_string());
            let trimmed = line.trim_start();
            if !inserted && (trimmed.starts_with("version =") || trimmed.starts_with("version=")) {
                let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                new_lines.push(format!("{indent}public_key = \"{pubkey}\","));
                inserted = true;
            }
        }
        if !inserted {
            new_lines.push(format!("  public_key = \"{pubkey}\","));
        }
    }

    let mut result = new_lines.join("\n");
    if content.ends_with('\n') {
        result.push('\n');
    }

    fs::write(manifest_path, result).map_err(|e| {
        format!(
            "failed to write manifest at '{}': {e}",
            manifest_path.display()
        )
    })?;
    Ok(())
}

pub fn pack(folder: &Path, key_path: Option<&Path>, out: Option<&Path>) -> Result<PathBuf, String> {
    let manifest_path = folder.join("manifest.lua");
    if !manifest_path.is_file() {
        return Err(format!("manifest.lua not found in '{}'", folder.display()));
    }

    let maybe_key = if let Some(key_path) = key_path {
        if let (Ok(folder_can), Ok(key_can)) = (folder.canonicalize(), key_path.canonicalize())
            && key_can.starts_with(&folder_can)
        {
            return Err(format!(
                "key file '{}' is inside the plugin folder '{}': refusing to pack private key",
                key_path.display(),
                folder.display()
            ));
        }
        let key = signing::read_key(key_path)
            .map_err(|e| format!("failed to read key file '{}': {e}", key_path.display()))?;
        let pubkey = PublicKey::from(&key);
        rewrite_manifest_public_key(&manifest_path, &pubkey.to_string())?;
        Some((key, pubkey))
    } else {
        None
    };

    let mut package =
        Package::from_dir(folder).map_err(|e| format!("failed to read plugin package: {e}"))?;
    let manifest_bytes = package
        .get("manifest.lua")
        .ok_or_else(|| "manifest.lua missing from package".to_string())?;
    let manifest_src = std::str::from_utf8(manifest_bytes)
        .map_err(|e| format!("invalid manifest encoding: {e}"))?;
    let manifest =
        manifest::evaluate(manifest_src).map_err(|e| format!("invalid manifest: {e}"))?;

    if let Some((ref key, ref pubkey)) = maybe_key {
        signing::sign(&mut package, &manifest, key).map_err(|e| format!("signing failed: {e}"))?;
        println!("{}", pubkey.fingerprint());
    }

    let default_name = format!("{}-{}.zip", manifest.id, manifest.version);
    let out_path = out
        .map(PathBuf::from)
        .unwrap_or_else(|| folder.join(default_name));

    let zip_bytes = package.write_zip();
    fs::write(&out_path, zip_bytes)
        .map_err(|e| format!("failed to write zip file '{}': {e}", out_path.display()))?;

    if maybe_key.is_none() {
        println!("Packed unsigned zip to {}", out_path.display());
    }
    Ok(out_path)
}

pub fn rotate_key(old: &Path, new: &Path, folder: &Path) -> Result<(), String> {
    let old_key = signing::read_key(old)
        .map_err(|e| format!("failed to read old key '{}': {e}", old.display()))?;
    let new_key = signing::read_key(new)
        .map_err(|e| format!("failed to read new key '{}': {e}", new.display()))?;
    let new_pubkey = PublicKey::from(&new_key);

    let rotation_content = signing::rotation_statement(&old_key, &new_pubkey);
    let rotation_path = folder.join("KEY-ROTATION");
    fs::write(&rotation_path, rotation_content)
        .map_err(|e| format!("failed to write KEY-ROTATION: {e}"))?;

    let manifest_path = folder.join("manifest.lua");
    if manifest_path.is_file() {
        rewrite_manifest_public_key(&manifest_path, &new_pubkey.to_string())?;
    }

    println!("Wrote KEY-ROTATION for {}", new_pubkey.fingerprint());
    Ok(())
}
