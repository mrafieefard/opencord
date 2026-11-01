//! macOS: `sysctl`.

use std::process::Command;

use super::gigabytes;

fn sysctl(name: &str) -> Option<String> {
    let output = Command::new("/usr/sbin/sysctl")
        .args(["-n", name])
        .output()
        .ok()?;
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

pub(super) fn basics() -> Vec<(String, String)> {
    let version = sysctl("kern.osproductversion").unwrap_or_else(|| "unknown".into());
    let build = sysctl("kern.osversion").unwrap_or_else(|| "unknown".into());
    let cpu = sysctl("machdep.cpu.brand_string").unwrap_or_else(|| std::env::consts::ARCH.into());
    let threads = std::thread::available_parallelism().map_or(0, |count| count.get());
    let memory = sysctl("hw.memsize")
        .and_then(|bytes| bytes.parse::<u64>().ok())
        .map_or_else(|| "unknown GB".to_owned(), gigabytes);
    vec![
        ("os".into(), format!("macOS {version} ({build})")),
        ("cpu".into(), format!("{cpu}, {threads} threads")),
        ("memory".into(), memory),
    ]
}

pub(super) fn graphics() -> Vec<(String, String)> {
    if cfg!(target_arch = "aarch64") {
        vec![("gpu 1".into(), "Apple (integrated)".into())]
    } else {
        Vec::new()
    }
}
