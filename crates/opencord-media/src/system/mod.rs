//! What the machine is, for the top of the session log (D82): the system
//! and its build, the processor, memory and graphics cards, the audio
//! devices, and what screen and sound sharing can use here.

use crate::audio::device::{self, DeviceList};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as platform;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod platform {
    pub(super) fn basics() -> Vec<(String, String)> {
        vec![
            ("os".into(), std::env::consts::OS.into()),
            ("cpu".into(), std::env::consts::ARCH.into()),
            ("memory".into(), "unknown GB".into()),
        ]
    }

    pub(super) fn graphics() -> Vec<(String, String)> {
        Vec::new()
    }
}

/// Quick to read, for the header: `os`, `cpu` and `memory`.
pub fn facts() -> Vec<(String, String)> {
    platform::basics()
}

/// Slower to read (graphics cards, audio devices, capture support), so
/// the session log writes them from a thread of their own once started.
pub fn machine() -> Vec<(String, String)> {
    let mut facts = platform::graphics();
    let screen = crate::video::screen::support();
    facts.push((
        "screen sharing".into(),
        match screen.reason {
            Some(reason) if !screen.supported => format!("no: {reason}"),
            _ => "yes".into(),
        },
    ));
    if let Some(works) = crate::video::screen::capture_api() {
        facts.push(("capture api".into(), yes_no(works).into()));
    }
    let sound = crate::share_audio::support();
    facts.push((
        "sound sharing".into(),
        match sound.reason {
            Some(reason) if !sound.supported => format!("no: {reason}"),
            _ => "yes".into(),
        },
    ));
    let (inputs, outputs) = device_lines(&device::devices());
    facts.push(("audio inputs".into(), inputs));
    facts.push(("audio outputs".into(), outputs));
    facts
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

/// Device names joined with `; `, the default marked.
fn device_lines(list: &DeviceList) -> (String, String) {
    let join = |devices: &[device::DeviceInfo], default: &Option<String>| {
        if devices.is_empty() {
            return "none".to_owned();
        }
        devices
            .iter()
            .map(|device| {
                if default.as_deref() == Some(device.id.as_str()) {
                    format!("{} (default)", device.name)
                } else {
                    device.name.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("; ")
    };
    (
        join(&list.inputs, &list.default_input),
        join(&list.outputs, &list.default_output),
    )
}

/// `17000000000` → `15.8 GB` (GiB, as systems show memory).
fn gigabytes(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / (1u64 << 30) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value<'a>(facts: &'a [(String, String)], key: &str) -> &'a str {
        facts
            .iter()
            .find(|(found, _)| found == key)
            .map(|(_, value)| value.as_str())
            .unwrap_or_else(|| panic!("no {key:?} in {facts:?}"))
    }

    #[test]
    fn facts_have_os_cpu_memory() {
        let facts = facts();
        assert!(!value(&facts, "os").is_empty());
        assert!(!value(&facts, "cpu").is_empty());
        assert!(value(&facts, "memory").ends_with(" GB"), "{facts:?}");
    }

    #[test]
    fn devices_say_which_is_default() {
        let list = crate::audio::device::DeviceList {
            inputs: vec![
                crate::audio::device::DeviceInfo {
                    id: "a".into(),
                    name: "Mic".into(),
                },
                crate::audio::device::DeviceInfo {
                    id: "b".into(),
                    name: "VoiceMeeter Input".into(),
                },
            ],
            outputs: vec![],
            default_input: Some("b".into()),
            default_output: None,
        };
        let (inputs, outputs) = device_lines(&list);
        assert_eq!(inputs, "Mic; VoiceMeeter Input (default)");
        assert_eq!(outputs, "none");
    }

    #[test]
    fn gigabytes_have_one_decimal() {
        assert_eq!(gigabytes(17_000_000_000), "15.8 GB");
    }
}
