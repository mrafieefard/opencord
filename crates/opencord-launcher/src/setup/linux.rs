//! Linux: a desktop file in `~/.local/share/applications`, which puts
//! Opencord in the desktop's app list and takes `opencord://` links. It is
//! the one the app writes for itself (opencord-media's `desktop_entry`),
//! marked the same way, with the launcher in `Exec`; the app, told where
//! its launcher is, leaves it pointing there.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::APP_ID;

const WRITTEN_BY_APP: &str = "X-Opencord-Written=true";

pub fn register(launcher: &Path) -> io::Result<()> {
    let Some(dir) = applications_dir() else {
        return Err(io::Error::other("no home folder"));
    };
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{APP_ID}.desktop"));
    let partial = path.with_extension("desktop.partial");
    fs::write(&partial, contents(launcher))?;
    fs::rename(&partial, &path)?;
    // Both are optional: without them the menu still finds the file, and
    // links ask which app to use the first time.
    quietly("update-desktop-database", &[dir.as_os_str()]);
    let desktop = format!("{APP_ID}.desktop");
    for scheme in ["opencord", "opencords"] {
        let mime = format!("x-scheme-handler/{scheme}");
        quietly(
            "xdg-mime",
            &["default".as_ref(), desktop.as_ref(), mime.as_ref()],
        );
    }
    Ok(())
}

/// Removes the desktop file if it is the one the launcher or the app
/// wrote; one a package installed stays.
pub fn unregister(launcher: &Path) {
    let _ = launcher;
    let Some(dir) = applications_dir() else {
        return;
    };
    let path = dir.join(format!("{APP_ID}.desktop"));
    if fs::read_to_string(&path)
        .is_ok_and(|text| text.lines().any(|line| line.trim() == WRITTEN_BY_APP))
    {
        let _ = fs::remove_file(&path);
        quietly("update-desktop-database", &[dir.as_os_str()]);
    }
    // Launch at login, which the app sets.
    if let Some(config) = config_dir() {
        let _ = fs::remove_file(config.join("autostart").join(format!("{APP_ID}.desktop")));
    }
}

fn applications_dir() -> Option<PathBuf> {
    env_dir("XDG_DATA_HOME", ".local/share").map(|dir| dir.join("applications"))
}

fn config_dir() -> Option<PathBuf> {
    env_dir("XDG_CONFIG_HOME", ".config")
}

fn env_dir(name: &str, under_home: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(under_home)))
}

fn quietly(program: &str, args: &[&std::ffi::OsStr]) {
    let _ = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// The app's desktop file (app/linux/packaging), launching `launcher`.
fn contents(launcher: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Opencord\n\
         GenericName=Chat\n\
         Comment=Self-hosted chat with voice\n\
         Exec={} %u\n\
         Icon={APP_ID}\n\
         Terminal=false\n\
         Categories=Network;InstantMessaging;Chat;\n\
         MimeType=x-scheme-handler/opencord;x-scheme-handler/opencords;\n\
         StartupWMClass={APP_ID}\n\
         StartupNotify=true\n\
         {WRITTEN_BY_APP}\n",
        exec_argument(&launcher.to_string_lossy()),
    )
}

/// A path as the desktop entry specification wants it in `Exec`: quoted
/// when it has to be, with `"`, `` ` ``, `$` and `\` escaped inside the
/// quotes, and `%` doubled.
fn exec_argument(path: &str) -> String {
    let path = path.replace('%', "%%");
    let reserved = |c: char| " \t\n\"'\\><~|&;$*?#()`".contains(c);
    if !path.chars().any(reserved) {
        return path;
    }
    let mut quoted = String::from("\"");
    for c in path.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_desktop_file_launches_the_launcher_and_takes_links() {
        let text = contents(Path::new("/home/a b/.local/share/opencord/opencord"));
        assert!(text.contains("Exec=\"/home/a b/.local/share/opencord/opencord\" %u\n"));
        assert!(text.contains("MimeType=x-scheme-handler/opencord;x-scheme-handler/opencords;\n"));
        assert!(text.contains(WRITTEN_BY_APP));
    }
}
