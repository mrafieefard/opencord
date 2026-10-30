//! Linux: `/etc/os-release`, `/proc` and the DRM devices in `/sys`.

use std::fs;

use super::gigabytes;

pub(super) fn basics() -> Vec<(String, String)> {
    let os_release = fs::read_to_string("/etc/os-release").unwrap_or_default();
    let name = os_release
        .lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))
        .map(|name| name.trim_matches('"').to_owned())
        .unwrap_or_else(|| "Linux".to_owned());
    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into());
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "unknown".into());
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let cpu = cpuinfo
        .lines()
        .find_map(|line| line.strip_prefix("model name"))
        .and_then(|rest| rest.split_once(':'))
        .map(|(_, model)| model.trim().to_owned())
        .unwrap_or_else(|| std::env::consts::ARCH.to_owned());
    let threads = std::thread::available_parallelism().map_or(0, |count| count.get());
    let memory = fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))
        .and_then(|rest| {
            rest.trim()
                .trim_end_matches("kB")
                .trim()
                .parse::<u64>()
                .ok()
        })
        .map_or_else(|| "unknown GB".to_owned(), |kb| gigabytes(kb * 1024));
    vec![
        ("os".into(), format!("{name}, kernel {}", kernel.trim())),
        ("session".into(), format!("{session}, {desktop}")),
        ("cpu".into(), format!("{cpu}, {threads} threads")),
        ("memory".into(), memory),
    ]
}

pub(super) fn graphics() -> Vec<(String, String)> {
    let Ok(cards) = fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    let mut names: Vec<String> = cards
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        // `card0`, not its outputs (`card0-HDMI-A-1`).
        .filter(|name| name.starts_with("card") && !name.contains('-'))
        .collect();
    names.sort();
    names
        .iter()
        .enumerate()
        .map(|(index, card)| {
            let uevent = fs::read_to_string(format!("/sys/class/drm/{card}/device/uevent"))
                .unwrap_or_default();
            let field = |key: &str| {
                uevent
                    .lines()
                    .find_map(|line| line.strip_prefix(key))
                    .unwrap_or("unknown")
                    .to_owned()
            };
            let driver = field("DRIVER=");
            let pci = field("PCI_ID=");
            let vendor = match pci.split(':').next().unwrap_or("") {
                "10DE" => "NVIDIA",
                "1002" => "AMD",
                "8086" => "Intel",
                _ => "unknown vendor",
            };
            let version = fs::read_to_string(format!("/sys/module/{driver}/version"))
                .map(|version| format!(" {}", version.trim()))
                .unwrap_or_default();
            (
                format!("gpu {}", index + 1),
                format!("{vendor} {pci}, driver {driver}{version}"),
            )
        })
        .collect()
}
