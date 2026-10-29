//! aws-lc's `sdallocx`, defined here on Android (decision D79).
//!
//! aws-lc (under rustls and the voice stack's DTLS) frees through
//! `sdallocx`, jemalloc's sized free, when the process has one: it declares
//! the name as a weak function and checks it for null. React Native's
//! `libreactnative.so` and `libjsi.so` export a *variable* of that name,
//! folly's null function pointer. Android's linker bound aws-lc's reference
//! to it, so the first free in aws-lc's random number generator (a voice
//! join's DTLS certificate) jumped into `.bss` and the app crashed.
//!
//! Defining it here, with `-Bsymbolic-functions` (build.rs), binds aws-lc's
//! reference to this function inside the library, which nothing loaded
//! later can replace.

use std::ffi::{c_int, c_void};

unsafe extern "C" {
    fn free(ptr: *mut c_void);
}

/// jemalloc's sized free: frees `ptr` like `free`; the size and flags are
/// hints the system allocator doesn't need.
///
/// # Safety
///
/// `ptr` is null or came from the system allocator and isn't freed yet, as
/// for `free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sdallocx(ptr: *mut c_void, _size: usize, _flags: c_int) {
    // SAFETY: the caller passes what `free` accepts (see above); aws-lc
    // passes memory it got from `malloc`.
    unsafe { free(ptr) }
}
