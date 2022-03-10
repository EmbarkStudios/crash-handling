//! Inline implementation of [] since it is not supported on all targets, namely
//! musl, as it has been deprecated from POSIX for over a decade
//!
//! The implementation is ported from Breakpad

extern "C" {
    pub fn crash_context_getcontext(ctx: *mut super::ucontext_t) -> i32;
}

cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        mod x86_64;
    } else if #[cfg(target_arch = "x86")] {
        mod x86;
    }
}
