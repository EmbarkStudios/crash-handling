//! [`CrashHandler`] provides a cross-platform way to handle crashes, executing
//! a user-specified function with the details of the crash when they occur.
//!
//! # Linux
//!
//! On Linux this is done by handling [signals](https://man7.org/linux/man-pages/man7/signal.7.html),
//! namely the following
//!
//! ## `SIGABRT`
//!
//! Signal sent to a process to tell it to abort, i.e. to terminate. The signal
//! is usually initiated by the process itself when it calls `std::process::abort`
//! or `libc::abort`, but it can be sent to the process from outside like any
//! other signal.
//!
//! ## `SIGBUS`
//!
//! Signal sent to a process when it causes a [bus error](https://en.wikipedia.org/wiki/Bus_error).
//!
//! ## `SIGFPE`
//!
//! Signal sent to a process when it executes an erroneous arithmetic operation.
//! Though it stands for **f**loating **p**oint **e**xception this signal covers
//! integer operations as well.
//!
//! ## `SIGILL`
//!
//! Signal sent to a process when it attempts to execute an **illegal**, malformed,
//! unknown, or privileged, instruction.
//!
//! ## `SIGSEGV`
//!
//! Signal sent to a process when it makes an invalid virtual memory reference,
//! a [segmentation fault](https://en.wikipedia.org/wiki/Segmentation_fault).
//! This covers infamous `null` pointer access, out of bounds access, use after
//! free, stack overflows, etc.
//!
//! ## `SIGTRAP`
//!
//! Signal sent to a process when a trap is raised, eg. a breakpoint or debug
//! assertion.
//!
//! One important detail of the Linux signal handling is that this crate hooks
//! [`pthread_create`](https://man7.org/linux/man-pages/man3/pthread_create.3.html)
//! so that an [alternate signal stack](https://man7.org/linux/man-pages/man2/sigaltstack.2.html)
//! is always installed on every thread. [`std::thread::Thread`] already does
//! this, however hooking `pthread_create` allows us to ensure this occurs for
//! threads created from eg. C/C++ code as well. An alternate stack is necessary
//! to reliably handle a `SIGSEGV` caused by a stack overflow as signals are
//! otherwise handled on the same stack that raised the signal.
//!
//! # Windows
//!
//! On Windows we catch [exceptions](https://docs.microsoft.com/en-us/windows/win32/debug/structured-exception-handling)
//! [invalid parameters](https://docs.microsoft.com/en-us/cpp/c-runtime-library/reference/set-invalid-parameter-handler-set-thread-local-invalid-parameter-handler?view=msvc-170)
//! and [purecall](https://docs.microsoft.com/en-us/cpp/c-runtime-library/reference/get-purecall-handler-set-purecall-handler?view=msvc-170)
//!
//! # Macos
//!
//! On Macos we use [exception ports](https://flylib.com/books/en/3.126.1.109/1/).
//! On Macos, exception ports are the first layer that exceptions are filtered,
//! from a thread level, to a process (task) level, and finally to a host level.
//!
//! If no user ports have been registered, the default Macos implementation is
//! to convert the Mach exception into an equivalent Unix signal and deliver it
//! to any registered signal handlers before performing the default action for
//! the exception/signal (ie process termination). This means that if you use
//! this crate in conjunction with signal handling on Macos, **you will not get
//! the results you expect** as the exception port used by this crate will take
//! precedence over the signal handler. See [this issue](
//! https://github.com/bytecodealliance/wasmtime/issues/2456) for a concrete
//! example.
//!
//! Note that there is one exception to the above, which is that `SIGABRT` is
//! handled by a signal handler, as there is no equivalent Mach exception for
//! it.

#![allow(unsafe_code)]

mod error;

pub use error::Error;

#[cfg(feature = "debug-print")]
#[macro_export]
macro_rules! debug_print {
    ($s:literal) => {
        let cstr = concat!($s, "\n");
        $crate::write_stderr(cstr);
    };
}

#[cfg(not(feature = "debug-print"))]
#[macro_export]
macro_rules! debug_print {
    ($s:literal) => {};
}

/// Writes the specified string directly to stderr.
///
/// This is safe to be called from within a compromised context.
#[inline]
pub fn write_stderr(s: &'static str) {
    unsafe {
        #[cfg(target_os = "windows")]
        libc::write(2, s.as_ptr().cast(), s.len() as u32);

        #[cfg(not(target_os = "windows"))]
        libc::write(2, s.as_ptr().cast(), s.len());
    }
}

cfg_if::cfg_if! {
    if #[cfg(all(unix, not(target_os = "macos")))] {
        /// The sole purpose of the unix module is to hook pthread_create to ensure
        /// an alternate stack is installed for every native thread in case of a
        /// stack overflow. This doesn't apply to MacOS as it uses exception ports,
        /// which are always delivered to a specific thread owned by the exception
        /// handler
        pub mod unix;
    }
}

pub use crash_context::CrashContext;

/// The result of the user code executed during a crash event
pub enum CrashEventResult {
    /// The event was handled in some way
    Handled(bool),
    #[cfg(not(target_os = "macos"))]
    /// The handler wishes to jump somewhere else, presumably to return
    /// execution and skip the code that caused the exception
    Jump {
        /// The location to jump back to, retrieved via sig/setjmp
        jmp_buf: *mut jmp::JmpBuf,
        /// The value that will be returned from the sig/setjmp call that we
        /// jump to. Note that if the value is 0 it will be corrected to 1
        value: i32,
    },
}

impl From<bool> for CrashEventResult {
    fn from(b: bool) -> Self {
        Self::Handled(b)
    }
}

/// User implemented trait for handling a crash event that has ocurred.
///
/// # Safety
///
/// This trait is marked unsafe as care needs to be taken when implementing it
/// due to the [`Self::on_crash`] method being run in a compromised context. In
/// general, it is advised to do as _little_ as possible when handling a
/// crash, with more complicated or dangerous (in a compromised context) code
/// being intialized before the [`CrashHandler`] is installed, or hoisted out to
/// another process entirely.
///
/// ## Linux
///
/// Notably, only a small subset of libc functions are
/// [async signal safe](https://man7.org/linux/man-pages/man7/signal-safety.7.html)
/// and calling non-safe ones can have undefined behavior, including such common
/// ones as `malloc` (especially if using a multi-threaded allocator).
///
/// ## Windows
///
/// Windows [structured exceptions](https://docs.microsoft.com/en-us/windows/win32/debug/structured-exception-handling)
/// don't have the a notion similar to signal safety, but it is again recommended
/// to do as little work as possible in response to an exception.
///
/// ## Macos
///
/// Mac uses exception ports (sorry, can't give a good link here since Apple
/// documentation is terrible) which are handled by a thread owned by the
/// exception handler which makes them slightly safer to handle than UNIX signals,
/// but it is again recommended to do as little work as possible.
pub unsafe trait CrashEvent: Send + Sync {
    /// Method invoked when a crash occurs. Returning true indicates your handler
    /// has processed the crash and that no further handlers should run.
    fn on_crash(&self, context: &CrashContext) -> CrashEventResult;
}

/// Creates a [`CrashEvent`] using the supplied closure as the implementation.
///
/// # Safety
///
/// See the [`CrashEvent`] Safety section for information on why this is `unsafe`.
#[inline]
pub unsafe fn make_crash_event<F>(closure: F) -> Box<dyn CrashEvent>
where
    F: Send + Sync + Fn(&CrashContext) -> CrashEventResult + 'static,
{
    struct Wrapper<F> {
        inner: F,
    }

    unsafe impl<F> CrashEvent for Wrapper<F>
    where
        F: Send + Sync + Fn(&CrashContext) -> CrashEventResult,
    {
        fn on_crash(&self, context: &CrashContext) -> CrashEventResult {
            (self.inner)(context)
        }
    }

    Box::new(Wrapper { inner: closure })
}

cfg_if::cfg_if! {
    if #[cfg(any(target_os = "linux", target_os = "android"))] {
        mod linux;

        pub use linux::{CrashHandler, Signal, jmp};
    } else if #[cfg(target_os = "windows")] {
        mod windows;

        pub use windows::{CrashHandler, ExceptionCode, jmp};
    } else if #[cfg(target_os = "macos")] {
        mod mac;

        pub use mac::{CrashHandler, ExceptionType};
    }
}
