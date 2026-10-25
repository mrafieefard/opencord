//! Windows: everything goes under the current user, so installing needs
//! no administrator: Start menu and desktop shortcuts, `opencord://` and
//! `opencords://` links, and an entry in Settings → Apps that runs
//! `Opencord.exe --uninstall`.
//!
//! The shortcuts carry the app's AppUserModelID, which the app sets on
//! itself too: Windows then treats the app as the shortcut's, so pinning
//! it to the taskbar pins the launcher, not one version's executable.

use std::ffi::c_void;
use std::io;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use windows::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, IPersistFile,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegDeleteTreeW, RegGetValueW, RegSetValueExW,
};
use windows::Win32::System::Variant::VT_LPWSTR;
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Programs, IShellLinkW, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
    SHStrDupW, ShellLink,
};
use windows::Win32::UI::WindowsAndMessaging::{IDYES, MB_ICONQUESTION, MB_YESNO, MessageBoxW};
use windows::core::{HSTRING, Interface, PCWSTR};

use super::APP_ID;
use crate::layout::Layout;

const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Opencord";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const SCHEMES: [&str; 2] = ["opencord", "opencords"];
const SHORTCUT: &str = "Opencord.lnk";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn register(layout: &Layout, launcher: &Path) -> io::Result<()> {
    let exe = launcher.display().to_string();
    for scheme in SCHEMES {
        let key = format!(r"Software\Classes\{scheme}");
        set_strings(&key, &[("", "URL:Opencord"), ("URL Protocol", "")])?;
        set_strings(
            &format!(r"{key}\shell\open\command"),
            &[("", &format!("\"{exe}\" \"%1\""))],
        )?;
    }
    let version = env!("CARGO_PKG_VERSION");
    set_strings(
        UNINSTALL_KEY,
        &[
            ("DisplayName", "Opencord"),
            ("DisplayVersion", version),
            ("DisplayIcon", &exe),
            ("Publisher", "Opencord"),
            ("InstallLocation", &layout.root().display().to_string()),
            ("UninstallString", &format!("\"{exe}\" --uninstall")),
            (
                "QuietUninstallString",
                &format!("\"{exe}\" --uninstall --quiet"),
            ),
        ],
    )?;
    set_dword(UNINSTALL_KEY, "NoModify", 1)?;
    set_dword(UNINSTALL_KEY, "NoRepair", 1)?;
    for folder in shortcut_folders() {
        // A missing desktop shortcut is no reason to fail the install.
        let _ = create_shortcut(&folder.join(SHORTCUT), launcher, layout.root());
    }
    Ok(())
}

pub fn uninstall(layout: &Layout) -> io::Result<()> {
    let quiet = std::env::args().any(|arg| arg == "--quiet");
    if !quiet && !confirm() {
        return Ok(());
    }
    let launcher = layout.launcher().display().to_string();
    for scheme in SCHEMES {
        let key = format!(r"Software\Classes\{scheme}");
        // Only if the links are still Opencord's from here.
        if read_string(&format!(r"{key}\shell\open\command"), "")
            .is_some_and(|command| command.contains(&launcher))
        {
            delete_tree(&key);
        }
    }
    delete_tree(UNINSTALL_KEY);
    delete_value(RUN_KEY, "Opencord");
    for folder in shortcut_folders() {
        let _ = std::fs::remove_file(folder.join(SHORTCUT));
    }
    // The running launcher can't remove itself: what it can is removed
    // now, and a shell that waits for it to end removes the rest.
    if let Ok(entries) = std::fs::read_dir(layout.root()) {
        for entry in entries.flatten() {
            let path = entry.path();
            let _ = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
        }
    }
    let root = layout.root().display().to_string();
    Command::new("cmd")
        .raw_arg(format!(
            "/d /c ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{root}\""
        ))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;
    Ok(())
}

fn confirm() -> bool {
    let text = HSTRING::from(
        "Remove Opencord from this computer?\n\nYour servers, messages and settings stay, \
         for when you install it again.",
    );
    // SAFETY: both strings outlive the call; no owner window.
    unsafe {
        MessageBoxW(
            None,
            &text,
            &HSTRING::from("Uninstall Opencord"),
            MB_YESNO | MB_ICONQUESTION,
        ) == IDYES
    }
}

fn shortcut_folders() -> Vec<PathBuf> {
    [FOLDERID_Programs, FOLDERID_Desktop]
        .iter()
        .filter_map(|folder| {
            // SAFETY: the returned string is copied, then freed with the
            // allocator the shell used.
            unsafe {
                let path = SHGetKnownFolderPath(folder, KF_FLAG_DEFAULT, None).ok()?;
                let text = path.to_string().ok();
                CoTaskMemFree(Some(path.0 as *const c_void));
                text.map(PathBuf::from)
            }
        })
        .collect()
}

fn create_shortcut(path: &Path, target: &Path, dir: &Path) -> windows::core::Result<()> {
    // SAFETY: COM is started on this thread before any object is made;
    // every interface is released when dropped; the property value owns
    // its string, which `PROPVARIANT`'s drop frees as the shell expects.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(&HSTRING::from(target.as_os_str()))?;
        link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?;
        link.SetDescription(&HSTRING::from("Opencord"))?;
        let store: IPropertyStore = link.cast()?;
        let mut id = PROPVARIANT::default();
        let inner = &mut *id.Anonymous.Anonymous;
        inner.vt = VT_LPWSTR;
        inner.Anonymous.pwszVal = SHStrDupW(&HSTRING::from(APP_ID))?;
        store.SetValue(&PKEY_AppUserModel_ID, &id)?;
        store.Commit()?;
        let file: IPersistFile = link.cast()?;
        file.Save(&HSTRING::from(path.as_os_str()), true)
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn check(error: WIN32_ERROR) -> io::Result<()> {
    if error == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(error.0 as i32))
    }
}

/// Opens (creating) `path` under the current user, runs `f` on it, and
/// closes it.
fn with_key(path: &str, f: impl FnOnce(HKEY) -> io::Result<()>) -> io::Result<()> {
    let path = wide(path);
    let mut key = HKEY::default();
    // SAFETY: `path` is NUL-terminated and outlives the call; the key is
    // closed below.
    unsafe {
        check(RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        ))?;
        let result = f(key);
        let _ = RegCloseKey(key);
        result
    }
}

/// Sets string values; `""` is the key's default value.
fn set_strings(path: &str, values: &[(&str, &str)]) -> io::Result<()> {
    with_key(path, |key| {
        for (name, value) in values {
            let name = wide(name);
            let data = wide(value);
            // SAFETY: the bytes are the NUL-terminated UTF-16 value.
            let bytes =
                unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2) };
            let name = if name.len() == 1 {
                PCWSTR::null()
            } else {
                PCWSTR(name.as_ptr())
            };
            // SAFETY: `name` and `bytes` outlive the call.
            check(unsafe { RegSetValueExW(key, name, None, REG_SZ, Some(bytes)) })?;
        }
        Ok(())
    })
}

fn set_dword(path: &str, name: &str, value: u32) -> io::Result<()> {
    with_key(path, |key| {
        let name = wide(name);
        // SAFETY: `name` and the value outlive the call.
        check(unsafe {
            RegSetValueExW(
                key,
                PCWSTR(name.as_ptr()),
                None,
                REG_DWORD,
                Some(&value.to_le_bytes()),
            )
        })
    })
}

fn read_string(path: &str, name: &str) -> Option<String> {
    let path = wide(path);
    let name = wide(name);
    let name = if name.len() == 1 {
        PCWSTR::null()
    } else {
        PCWSTR(name.as_ptr())
    };
    let mut buffer = vec![0u16; 2048];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: the buffer and its size agree; the strings outlive the call.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            name,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    check(result).ok()?;
    let length = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(
        &buffer[..length.min(buffer.len())],
    ))
}

fn delete_tree(path: &str) {
    let path = wide(path);
    // SAFETY: `path` is NUL-terminated and outlives the call.
    let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr())) };
    // `RegDeleteTreeW` empties the key; this removes it.
    let _ = unsafe {
        windows::Win32::System::Registry::RegDeleteKeyW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr()))
    };
}

fn delete_value(path: &str, name: &str) {
    let path = wide(path);
    let name = wide(name);
    // SAFETY: both are NUL-terminated and outlive the call.
    let _ = unsafe {
        RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            PCWSTR(name.as_ptr()),
        )
    };
}
