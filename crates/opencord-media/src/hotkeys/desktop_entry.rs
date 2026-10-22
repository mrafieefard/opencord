//! The app's desktop file, which Wayland desktops need before they bind
//! global shortcuts: the portal finds an app outside a sandbox by its
//! desktop file, and refuses one it can't find ("App info not found").
//! A package installs the file; the tarball doesn't, so Opencord writes
//! its own under `~/.local/share/applications`, pointing at wherever it
//! runs from, as Telegram's tarball does.

use std::io;
use std::path::{Path, PathBuf};

/// Marks a desktop file Opencord wrote itself, which it may rewrite when
/// the app moves; one a package installed is left alone.
const WRITTEN_BY_APP: &str = "X-Opencord-Written=true";

/// What happened to the desktop file.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Entry {
    /// A package, or the user, installed one; it is used as it is.
    Installed,
    /// Opencord's own is there and up to date.
    Current,
    /// Opencord wrote its own just now.
    Written,
}

/// Makes sure the desktop can find `app_id`'s desktop file, writing
/// Opencord's own for `executable` when no other is installed.
pub(super) fn ensure(app_id: &str, executable: &Path) -> io::Result<Entry> {
    ensure_in(&data_dirs(), app_id, executable)
}

/// [`ensure`], with the data directories given: the user's first, where
/// a written file goes.
fn ensure_in(dirs: &[PathBuf], app_id: &str, executable: &Path) -> io::Result<Entry> {
    let Some((user, _)) = dirs.split_first() else {
        return Err(io::Error::other("no data directory"));
    };
    let name = format!("applications/{app_id}.desktop");
    let wanted = contents(app_id, executable);
    for dir in dirs {
        let path = dir.join(&name);
        let Ok(found) = std::fs::read_to_string(&path) else {
            continue;
        };
        if !found.lines().any(|line| line.trim() == WRITTEN_BY_APP) {
            return Ok(Entry::Installed);
        }
        if found == wanted {
            return Ok(Entry::Current);
        }
        // Ours, from where the app was before: it is rewritten below.
        break;
    }
    let path = user.join(&name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Whole or not at all: the desktop may read it as it is written.
    let partial = path.with_extension("desktop.partial");
    std::fs::write(&partial, wanted)?;
    std::fs::rename(&partial, &path)?;
    Ok(Entry::Written)
}

/// `$XDG_DATA_HOME` (or `~/.local/share`), then `$XDG_DATA_DIRS` (or
/// `/usr/local/share` and `/usr/share`), as the desktop looks in them.
fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
    home.into_iter()
        .chain(
            system
                .split(':')
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from),
        )
        .collect()
}

/// The packaged desktop file (app/linux/packaging), launching
/// `executable`.
fn contents(app_id: &str, executable: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Opencord\n\
         GenericName=Chat\n\
         Comment=Self-hosted chat with voice\n\
         Exec={} %u\n\
         Icon={app_id}\n\
         Terminal=false\n\
         Categories=Network;InstantMessaging;Chat;\n\
         MimeType=x-scheme-handler/opencord;\n\
         StartupWMClass={app_id}\n\
         StartupNotify=true\n\
         {WRITTEN_BY_APP}\n",
        exec_argument(&executable.to_string_lossy()),
    )
}

/// A path as the desktop entry specification wants it in `Exec`: quoted
/// when it has to be, with `"`, `` ` ``, `$` and `\` escaped inside the
/// quotes, and `%` doubled, as field codes start with it.
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

    const ID: &str = "dev.opencord.opencord";

    fn dirs(root: &Path) -> Vec<PathBuf> {
        vec![root.join("home"), root.join("usr")]
    }

    fn read(dir: &Path) -> String {
        std::fs::read_to_string(dir.join(format!("applications/{ID}.desktop"))).unwrap()
    }

    #[test]
    fn without_a_desktop_file_the_app_writes_its_own_once() {
        let root = tempfile::tempdir().unwrap();
        let dirs = dirs(root.path());
        let app = Path::new("/opt/opencord/bundle/opencord");

        let first = ensure_in(&dirs, ID, app).unwrap();
        let second = ensure_in(&dirs, ID, app).unwrap();

        assert_eq!(first, Entry::Written);
        assert_eq!(second, Entry::Current);
        let written = read(&dirs[0]);
        assert!(written.contains("Exec=/opt/opencord/bundle/opencord %u\n"));
        assert!(written.contains(&format!("StartupWMClass={ID}\n")));
    }

    #[test]
    fn a_packaged_desktop_file_is_left_alone() {
        let root = tempfile::tempdir().unwrap();
        let dirs = dirs(root.path());
        let applications = dirs[1].join("applications");
        std::fs::create_dir_all(&applications).unwrap();
        std::fs::write(applications.join(format!("{ID}.desktop")), "packaged").unwrap();

        let entry = ensure_in(&dirs, ID, Path::new("/somewhere/opencord")).unwrap();

        assert_eq!(entry, Entry::Installed);
        assert!(!dirs[0].join("applications").exists());
    }

    #[test]
    fn the_app_moving_rewrites_its_own_desktop_file() {
        let root = tempfile::tempdir().unwrap();
        let dirs = dirs(root.path());
        ensure_in(&dirs, ID, Path::new("/old/opencord")).unwrap();

        let entry = ensure_in(&dirs, ID, Path::new("/new place/opencord")).unwrap();

        assert_eq!(entry, Entry::Written);
        assert!(read(&dirs[0]).contains("Exec=\"/new place/opencord\" %u\n"));
    }

    #[test]
    fn exec_paths_are_quoted_and_escaped_as_the_specification_says() {
        assert_eq!(exec_argument("/usr/bin/opencord"), "/usr/bin/opencord");
        assert_eq!(
            exec_argument("/home/a b/opencord"),
            "\"/home/a b/opencord\""
        );
        assert_eq!(exec_argument("/x/$HOME/o"), "\"/x/\\$HOME/o\"");
        assert_eq!(exec_argument("/x/100%/o"), "/x/100%%/o");
    }

    #[test]
    fn the_written_file_matches_the_packaged_one_but_for_exec() {
        let packaged = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../app/linux/packaging/dev.opencord.opencord.desktop"
        ));
        // The server's image has no app/ to compare with.
        let Ok(packaged) = packaged else { return };
        let written = contents(ID, Path::new("/x/opencord"));
        let lines = |text: &str| -> Vec<String> {
            text.lines()
                .filter(|line| !line.starts_with("Exec=") && *line != WRITTEN_BY_APP)
                .map(str::to_owned)
                .collect()
        };

        assert_eq!(lines(&written), lines(&packaged));
    }
}
