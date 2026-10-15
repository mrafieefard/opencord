//! Windows: the time since the last input event in this session
//! (`GetLastInputInfo`), against the tick count it is measured in.

use std::time::Duration;

use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

pub async fn idle_time() -> Option<Duration> {
    let mut info = LASTINPUTINFO {
        cbSize: u32::try_from(std::mem::size_of::<LASTINPUTINFO>()).ok()?,
        dwTime: 0,
    };
    // SAFETY: `info` is a valid LASTINPUTINFO with its size set, as the
    // call requires; it only writes `dwTime`.
    if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
        return None;
    }
    // SAFETY: no arguments; reads the system's tick count.
    let now = unsafe { GetTickCount() };
    // Both wrap every 49.7 days; the difference does not.
    Some(Duration::from_millis(u64::from(
        now.wrapping_sub(info.dwTime),
    )))
}
