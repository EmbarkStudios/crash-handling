//! Macos doesn't have an exception for process aborts, so we hook `SIGABRT`
use std::mem;

/// Installs our `SIGABRT` handler, returning any previously registered handler,
/// which should be restored later
///
/// SAFETY: syscall
pub(crate) unsafe fn install_abort_handler() -> Result<libc::sigaction, std::io::Error> {
    let mut sa: libc::sigaction = mem::zeroed();
    libc::sigemptyset(&mut sa.sa_mask);
    libc::sigaddset(&mut sa.sa_mask, libc::SIGABRT);
    sa.sa_sigaction = signal_handler as usize;
    sa.sa_flags = libc::SA_SIGINFO;

    let mut old_action = mem::MaybeUninit::uninit();

    if libc::sigaction(libc::SIGABRT, &sa, old_action.as_mut_ptr()) != -1 {
        Ok(old_action.assume_init())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Restores the action for `SIGABRT` to the specified handler
///
/// SAFETY: syscall
#[inline]
pub(crate) unsafe fn restore_abort_handler(handler: libc::sigaction) {
    libc::sigaction(libc::SIGABRT, &handler, std::ptr::null_mut());
}

/// Our signal handler
unsafe extern "C" fn signal_handler(
    signal: i32,
    _info: *mut libc::siginfo_t,
    _uc: *mut std::ffi::c_void,
) {
    use super::ffi;

    // Sanity check
    assert_eq!(signal, libc::SIGABRT);

    super::state::simulate_exception(Some(crash_context::ExceptionInfo {
        kind: ffi::et::EXC_SOFTWARE as i32, // 5
        code: ffi::EXC_SOFT_SIGNAL as _,    // Unix signal
        subcode: Some(signal as _),
    }));
}
