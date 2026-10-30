//! Panics and native crashes in the session log (D82).
//!
//! A panic goes through the writer like any line, with its backtrace, and
//! is flushed before the previous hook runs. A native crash can't use the
//! writer: the handler formats one line on the stack and writes it
//! straight to the file, then hands the crash on to whoever handled it
//! before. Windows' fail-fast crashes (heap corruption, `0xC0000374`)
//! skip every handler in the process; those sessions show only as having
//! ended unexpectedly.

use std::backtrace::Backtrace;
use std::fmt::Write as _;
use std::fs::File;
use std::io::Write as _;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use tracing::Level;

/// Backtrace lines kept from a panic.
const FRAMES: usize = 60;

/// The session's file, opened a second time for crash handlers, and when
/// the session started.
static CRASH: OnceLock<(File, Instant)> = OnceLock::new();

pub(super) fn install(file: File, started: Instant) {
    if CRASH.set((file, started)).is_err() {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("(no message)");
        let location = info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_default();
        let thread = std::thread::current();
        let fields = format!(
            " thread={} location={location}",
            thread.name().unwrap_or("unnamed")
        );
        super::send(Level::ERROR, "panic", message, &fields);
        let mut trace = String::from("backtrace:");
        for line in Backtrace::force_capture().to_string().lines().take(FRAMES) {
            let _ = write!(trace, "\n{line}");
        }
        super::send(Level::ERROR, "panic", &trace, "");
        super::flush();
        previous(info);
    }));
    native::install();
}

/// Writes `bytes` to the session's file without the writer.
pub(super) fn write_direct(bytes: &[u8]) {
    if let Some((file, _)) = CRASH.get() {
        let _ = (&*file).write_all(bytes);
    }
}

pub(crate) enum Native<'a> {
    #[cfg_attr(not(windows), allow(dead_code))]
    Exception {
        code: u32,
        module: &'a [u8],
        offset: usize,
    },
    #[cfg_attr(windows, allow(dead_code))]
    Signal(i32),
}

/// A fixed-size buffer that drops what doesn't fit, so formatting never
/// allocates.
struct Stack<'a> {
    buf: &'a mut [u8; 256],
    length: usize,
}

impl std::fmt::Write for Stack<'_> {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        let room = self.buf.len() - self.length;
        let taken = text.len().min(room);
        self.buf[self.length..self.length + taken].copy_from_slice(&text.as_bytes()[..taken]);
        self.length += taken;
        Ok(())
    }
}

/// The crash's line in `buf`; returns its length. Always ends in a
/// newline, cut short if it must be.
pub(crate) fn format_native(buf: &mut [u8; 256], elapsed: Duration, native: Native<'_>) -> usize {
    let mut out = Stack { buf, length: 0 };
    let _ = write!(
        out,
        "+{}.{:03}s FATAL native crash  ",
        elapsed.as_secs(),
        elapsed.subsec_millis()
    );
    match native {
        Native::Exception {
            code,
            module,
            offset,
        } => {
            let _ = write!(out, "code=0x{code:08X} module=");
            for &byte in module {
                let shown = if byte.is_ascii_graphic() { byte } else { b'?' };
                let _ = out.write_char(char::from(shown));
            }
            let _ = write!(out, " offset=0x{offset:x}");
        }
        Native::Signal(signal) => {
            let _ = write!(out, "signal={signal} ({})", signal_name(signal));
        }
    }
    let length = out.length.min(255);
    buf[length] = b'\n';
    length + 1
}

/// The line a signal that ends the app (logging out, `kill`) writes: the
/// session ended, it didn't crash.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn format_ended(buf: &mut [u8; 256], elapsed: Duration, signal: i32) -> usize {
    let mut out = Stack { buf, length: 0 };
    let _ = write!(
        out,
        "+{}.{:03}s INFO  log  {} (signal {signal})",
        elapsed.as_secs(),
        elapsed.subsec_millis(),
        super::writer::ENDED
    );
    let length = out.length.min(255);
    buf[length] = b'\n';
    length + 1
}

fn signal_name(signal: i32) -> &'static str {
    #[cfg(unix)]
    {
        match signal {
            libc::SIGSEGV => "SIGSEGV",
            libc::SIGBUS => "SIGBUS",
            libc::SIGILL => "SIGILL",
            libc::SIGFPE => "SIGFPE",
            libc::SIGABRT => "SIGABRT",
            _ => "signal",
        }
    }
    #[cfg(not(unix))]
    {
        let _ = signal;
        "signal"
    }
}

#[cfg(unix)]
mod native {
    use std::os::fd::AsRawFd;
    use std::sync::OnceLock;

    use super::{CRASH, Native, format_ended, format_native};

    /// Crashes, then the signals that end the app without one.
    const SIGNALS: [libc::c_int; 8] = [
        libc::SIGSEGV,
        libc::SIGBUS,
        libc::SIGILL,
        libc::SIGFPE,
        libc::SIGABRT,
        libc::SIGTERM,
        libc::SIGINT,
        libc::SIGHUP,
    ];
    const ENDING: [libc::c_int; 3] = [libc::SIGTERM, libc::SIGINT, libc::SIGHUP];

    /// What each signal did before, to hand it back to.
    static PREVIOUS: OnceLock<[libc::sigaction; 8]> = OnceLock::new();

    pub(super) fn install() {
        // SAFETY: sigaction with a null new action only reads the current
        // one into `previous`, which outlives the call.
        let previous = SIGNALS.map(|signal| unsafe {
            let mut previous: libc::sigaction = std::mem::zeroed();
            libc::sigaction(signal, std::ptr::null(), &mut previous);
            previous
        });
        if PREVIOUS.set(previous).is_err() {
            return;
        }
        for signal in SIGNALS {
            // SAFETY: `on_signal` only reads statics set before this point,
            // formats on the stack and calls write and sigaction, all
            // async-signal-safe.
            unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = on_signal as *const () as usize;
                action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
                libc::sigemptyset(&mut action.sa_mask);
                libc::sigaction(signal, &action, std::ptr::null_mut());
            }
        }
    }

    extern "C" fn on_signal(signal: libc::c_int, info: *mut libc::siginfo_t, _: *mut libc::c_void) {
        if let Some((file, started)) = CRASH.get() {
            let mut buf = [0u8; 256];
            let length = if ENDING.contains(&signal) {
                format_ended(&mut buf, started.elapsed(), signal)
            } else {
                format_native(&mut buf, started.elapsed(), Native::Signal(signal))
            };
            // SAFETY: `buf` holds `length` bytes; write is async-signal-safe.
            unsafe { libc::write(file.as_raw_fd(), buf.as_ptr().cast(), length) };
        }
        // Sent rather than faulted: Linux numbers those 0 and below, macOS
        // from 0x10001 (SI_USER); faults are small positive codes on both.
        // SAFETY: the kernel passes a valid siginfo with SA_SIGINFO.
        let sent =
            unsafe { info.as_ref() }.is_none_or(|info| info.si_code <= 0 || info.si_code > 0x10000);
        let (Some(previous), Some(index)) = (
            PREVIOUS.get(),
            SIGNALS.iter().position(|&known| known == signal),
        ) else {
            return;
        };
        // SAFETY: sigaction and raise are async-signal-safe, and each
        // action set is one sigaction returned or the default.
        unsafe {
            if ENDING.contains(&signal) {
                // Whoever handled it before (a graceful shutdown) gets it.
                libc::sigaction(signal, &previous[index], std::ptr::null_mut());
                libc::raise(signal);
            } else if sent {
                // `kill -SEGV`, `abort`: nothing faulted, so returning would
                // let the process carry on, and the old handler (Rust's
                // stack-overflow one) handles only faults. The default ends
                // it.
                let mut default: libc::sigaction = std::mem::zeroed();
                default.sa_sigaction = libc::SIG_DFL;
                libc::sigaction(signal, &default, std::ptr::null_mut());
                libc::raise(signal);
            } else {
                // A real fault comes again as the instruction re-runs, under
                // the old handler.
                libc::sigaction(signal, &previous[index], std::ptr::null_mut());
            }
        }
    }
}

#[cfg(windows)]
mod native {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;

    use windows::Win32::Foundation::{HANDLE, HMODULE};
    use windows::Win32::Storage::FileSystem::WriteFile;
    use windows::Win32::System::Diagnostics::Debug::{
        EXCEPTION_POINTERS, LPTOP_LEVEL_EXCEPTION_FILTER, SetUnhandledExceptionFilter,
    };
    use windows::Win32::System::LibraryLoader::{
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        GetModuleFileNameW, GetModuleHandleExW,
    };
    use windows::core::PCWSTR;

    use super::{CRASH, Native, format_native};

    /// `EXCEPTION_CONTINUE_SEARCH`: let Windows (or the next filter) carry on.
    const CONTINUE_SEARCH: i32 = 0;

    static PREVIOUS: OnceLock<LPTOP_LEVEL_EXCEPTION_FILTER> = OnceLock::new();

    pub(super) fn install() {
        // SAFETY: installs a filter that lives for the whole process.
        let previous = unsafe { SetUnhandledExceptionFilter(Some(on_exception)) };
        let _ = PREVIOUS.set(previous);
    }

    unsafe extern "system" fn on_exception(info: *const EXCEPTION_POINTERS) -> i32 {
        // SAFETY: Windows passes valid exception pointers to the filter.
        if let (Some((file, started)), Some(record)) = (
            CRASH.get(),
            unsafe { info.as_ref() }.and_then(|info| unsafe { info.ExceptionRecord.as_ref() }),
        ) {
            let address = record.ExceptionAddress as usize;
            let mut module = HMODULE::default();
            let mut name = [0u16; 260];
            let mut length = 0;
            // SAFETY: looks up the module holding `address` without taking
            // a reference; `name` is a writable buffer.
            if unsafe {
                GetModuleHandleExW(
                    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                        | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                    PCWSTR(address as *const u16),
                    &mut module,
                )
            }
            .is_ok()
            {
                length = unsafe { GetModuleFileNameW(Some(module), &mut name) } as usize;
            }
            let path = &name[..length.min(name.len())];
            let start = path
                .iter()
                .rposition(|&unit| unit == u16::from(b'\\'))
                .map_or(0, |at| at + 1);
            let mut base = [0u8; 64];
            let mut base_length = 0;
            for &unit in path[start..].iter().take(base.len()) {
                base[base_length] = u8::try_from(unit).unwrap_or(b'?');
                base_length += 1;
            }
            let mut buf = [0u8; 256];
            let written = format_native(
                &mut buf,
                started.elapsed(),
                Native::Exception {
                    code: record.ExceptionCode.0 as u32,
                    module: &base[..base_length],
                    offset: address.wrapping_sub(module.0 as usize),
                },
            );
            // SAFETY: writes `written` bytes of `buf` to the session's file.
            let _ = unsafe {
                WriteFile(
                    HANDLE(file.as_raw_handle()),
                    Some(&buf[..written]),
                    None,
                    None,
                )
            };
        }
        match PREVIOUS.get().copied().flatten() {
            // SAFETY: the filter that was installed before ours.
            Some(previous) => unsafe { previous(info) },
            None => CONTINUE_SEARCH,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn crash_line_is_written_without_allocating() {
        let mut buf = [0u8; 256];
        let length = format_native(
            &mut buf,
            Duration::from_millis(12_345),
            Native::Exception {
                code: 0xC000_0005,
                module: b"ntdll.dll",
                offset: 0x1234,
            },
        );
        assert_eq!(
            std::str::from_utf8(&buf[..length]).unwrap(),
            "+12.345s FATAL native crash  code=0xC0000005 module=ntdll.dll offset=0x1234\n"
        );
        let length = format_native(&mut buf, Duration::from_millis(5), Native::Signal(11));
        assert_eq!(
            std::str::from_utf8(&buf[..length]).unwrap(),
            "+0.005s FATAL native crash  signal=11 (SIGSEGV)\n"
        );
    }

    #[test]
    fn a_long_module_name_is_cut_not_overflowed() {
        let mut buf = [0u8; 256];
        let module = [b'a'; 400];
        let length = format_native(
            &mut buf,
            Duration::ZERO,
            Native::Exception {
                code: 1,
                module: &module,
                offset: 0,
            },
        );
        assert_eq!(length, 256);
        assert_eq!(buf[255], b'\n');
    }

    #[test]
    fn panic_is_logged() {
        let session = super::super::tests::test_session();
        let result = std::panic::catch_unwind(|| panic!("boom in a test"));
        assert!(result.is_err());
        super::super::flush();
        let text = std::fs::read_to_string(&session.current).unwrap();
        assert!(text.contains("ERROR panic  boom in a test"), "{text}");
        assert!(
            text.contains("location=crates/opencord-core/src/log/crash.rs"),
            "{text}"
        );
    }

    /// Runs itself again as a child that crashes, then reads the child's
    /// session.
    #[cfg(unix)]
    #[test]
    fn native_crash_is_logged() {
        if let Ok(dir) = std::env::var("OPENCORD_CRASH_TEST_DIR") {
            super::super::start(std::path::Path::new(&dir), &[]).unwrap();
            // SAFETY: none; it deliberately crashes this child process.
            unsafe { std::ptr::null_mut::<u8>().write_volatile(1) };
            unreachable!("the signal ends the process");
        }
        let dir = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "log::crash::tests::native_crash_is_logged",
                "--nocapture",
            ])
            .env("OPENCORD_CRASH_TEST_DIR", dir.path())
            .status()
            .unwrap();
        assert!(!status.success());
        let file = std::fs::read_dir(dir.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        let text = std::fs::read_to_string(file.path()).unwrap();
        assert!(
            text.contains("FATAL native crash  signal=11 (SIGSEGV)"),
            "{text}"
        );
    }

    /// A crash signal sent from outside (`kill -SEGV`) faults nothing, so
    /// returning from the handler would let the process carry on.
    #[cfg(unix)]
    #[test]
    fn a_sent_crash_signal_still_ends_the_process() {
        use std::os::unix::process::ExitStatusExt;

        if let Ok(dir) = std::env::var("OPENCORD_SENT_SIGNAL_DIR") {
            super::super::start(std::path::Path::new(&dir), &[]).unwrap();
            // SAFETY: signals this process, as `kill` would.
            unsafe { libc::kill(libc::getpid(), libc::SIGSEGV) };
            std::thread::sleep(std::time::Duration::from_millis(200));
            std::process::exit(0);
        }
        let dir = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "log::crash::tests::a_sent_crash_signal_still_ends_the_process",
            ])
            .env("OPENCORD_SENT_SIGNAL_DIR", dir.path())
            .status()
            .unwrap();
        assert_eq!(status.signal(), Some(libc::SIGSEGV), "{status:?}");
    }

    /// Logging out or `kill` ends the app with SIGTERM: not a crash.
    #[cfg(unix)]
    #[test]
    fn sigterm_ends_the_session_cleanly() {
        use std::os::unix::process::ExitStatusExt;

        if let Ok(dir) = std::env::var("OPENCORD_SIGTERM_DIR") {
            super::super::start(std::path::Path::new(&dir), &[]).unwrap();
            // SAFETY: signals this process, as logging out would.
            unsafe { libc::kill(libc::getpid(), libc::SIGTERM) };
            std::thread::sleep(std::time::Duration::from_millis(500));
            std::process::exit(0);
        }
        let dir = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "log::crash::tests::sigterm_ends_the_session_cleanly",
            ])
            .env("OPENCORD_SIGTERM_DIR", dir.path())
            .status()
            .unwrap();
        assert_eq!(status.signal(), Some(libc::SIGTERM), "{status:?}");
        let file = std::fs::read_dir(dir.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        let text = std::fs::read_to_string(file.path()).unwrap();
        assert!(text.contains("session ended (signal 15)"), "{text}");
    }
}
