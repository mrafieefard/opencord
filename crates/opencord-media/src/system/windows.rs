//! Windows: the registry's version keys, DXGI's adapters and
//! `GlobalMemoryStatusEx`.

use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIDevice, IDXGIFactory1,
};
use windows::Win32::System::Registry::{
    HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::core::{HSTRING, Interface};

use super::gigabytes;

const VERSION_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
const CPU_KEY: &str = r"HARDWARE\DESCRIPTION\System\CentralProcessor\0";
/// Windows 11 is build 22000 and later, though the registry still says 10.
const WINDOWS_11: u32 = 22_000;

fn text(key: &str, value: &str) -> Option<String> {
    let mut buffer = [0u16; 256];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: `buffer` holds `size` bytes, and RegGetValueW writes at most
    // that, NUL included.
    unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(key),
            &HSTRING::from(value),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    }
    .ok()
    .ok()?;
    let units = (size as usize / 2).saturating_sub(1).min(buffer.len());
    let text = String::from_utf16_lossy(&buffer[..units]).trim().to_owned();
    (!text.is_empty()).then_some(text)
}

fn number(key: &str, value: &str) -> Option<u32> {
    let mut number = 0u32;
    let mut size = 4u32;
    // SAFETY: a DWORD value into a u32 of the size given.
    unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(key),
            &HSTRING::from(value),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut number as *mut u32).cast()),
            Some(&mut size),
        )
    }
    .ok()
    .ok()?;
    Some(number)
}

pub(super) fn basics() -> Vec<(String, String)> {
    let build: u32 = text(VERSION_KEY, "CurrentBuildNumber")
        .and_then(|build| build.parse().ok())
        .unwrap_or(0);
    let mut product = text(VERSION_KEY, "ProductName").unwrap_or_else(|| "Windows".into());
    if build >= WINDOWS_11 {
        product = product.replace("Windows 10", "Windows 11");
    }
    let release = text(VERSION_KEY, "DisplayVersion")
        .or_else(|| text(VERSION_KEY, "ReleaseId"))
        .unwrap_or_default();
    let revision = number(VERSION_KEY, "UBR").unwrap_or(0);
    let edition = text(VERSION_KEY, "EditionID").unwrap_or_default();
    let cpu = text(CPU_KEY, "ProcessorNameString").unwrap_or_else(|| "unknown".into());
    let threads = std::thread::available_parallelism().map_or(0, |count| count.get());
    let mut memory = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: `memory` is sized as the call requires.
    let memory = match unsafe { GlobalMemoryStatusEx(&mut memory) } {
        Ok(()) => gigabytes(memory.ullTotalPhys),
        Err(_) => "unknown GB".to_owned(),
    };
    vec![
        (
            "os".into(),
            format!("{product} {release} ({build}.{revision}), edition {edition}"),
        ),
        ("cpu".into(), format!("{cpu}, {threads} threads")),
        ("memory".into(), memory),
    ]
}

pub(super) fn graphics() -> Vec<(String, String)> {
    // SAFETY: creating a DXGI factory has no preconditions.
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return Vec::new();
    };
    let mut facts = Vec::new();
    for index in 0.. {
        // SAFETY: enumeration ends with an error past the last adapter.
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(index) }) else {
            break;
        };
        // SAFETY: a description of an adapter DXGI just returned.
        let Ok(description) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        // The Basic Render Driver.
        if description.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
            continue;
        }
        let end = description
            .Description
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(description.Description.len());
        let name = String::from_utf16_lossy(&description.Description[..end]);
        // SAFETY: asks for the user-mode driver's version.
        let driver = match unsafe { adapter.CheckInterfaceSupport(&IDXGIDevice::IID) } {
            Ok(version) => {
                let version = version as u64;
                format!(
                    "{}.{}.{}.{}",
                    version >> 48,
                    (version >> 32) & 0xffff,
                    (version >> 16) & 0xffff,
                    version & 0xffff
                )
            }
            Err(_) => "unknown".into(),
        };
        facts.push((
            format!("gpu {}", facts.len() + 1),
            format!(
                "{name}, driver {driver}, {} MB video memory",
                description.DedicatedVideoMemory / (1 << 20)
            ),
        ));
    }
    facts
}
