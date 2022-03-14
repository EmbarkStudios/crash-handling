mod state;

use crate::{CrashContext, Error};

/// User implemented trait for handling a signal that has ocurred.
///
/// # Safety
///
/// This trait is marked unsafe as  care needs to be taken when implementing it
/// due to running in a compromised context. Notably, only a small subset of
/// libc functions are [async signal safe](https://man7.org/linux/man-pages/man7/signal-safety.7.html)
/// and calling non-safe ones can have undefined behavior, including such common
/// ones as `malloc` (if using a multi-threaded allocator). In general, it is
/// advised to do as _little_ as possible when handling a signal, with more
/// complicated or dangerous (in a compromised context) code being intialized
/// before the signal handler is installed, or hoisted out to an entirely
/// different sub-process.
pub unsafe trait CrashEvent: Send + Sync {
    /// Method invoked when a crash occurs. Returning true indicates your handler
    /// has processed the crash and that no further handlers should run.
    fn on_crash(&self, context: &CrashContext) -> bool;
}

/// Creates a [`CrashEvent`] using the supplied closure as the implementation.
///
/// # Safety
///
/// See the [`CrashEvent`] Safety section for information on why this is `unsafe`.
#[inline]
pub unsafe fn make_crash_event<F>(closure: F) -> Box<dyn CrashEvent>
where
    F: Send + Sync + Fn(&CrashContext) -> bool + 'static,
{
    struct Wrapper<F> {
        inner: F,
    }

    unsafe impl<F> CrashEvent for Wrapper<F>
    where
        F: Send + Sync + Fn(&CrashContext) -> bool,
    {
        fn on_crash(&self, context: &CrashContext) -> bool {
            (self.inner)(context)
        }
    }

    Box::new(Wrapper { inner: closure })
}

#[derive(Copy, Clone, PartialEq)]
#[repr(i32)]
pub enum Signal {
    Hup = libc::SIGHUP,
    Int = libc::SIGINT,
    Quit = libc::SIGQUIT,
    Ill = libc::SIGILL,
    Trap = libc::SIGTRAP,
    Abort = libc::SIGABRT,
    Bus = libc::SIGBUS,
    Fpe = libc::SIGFPE,
    Kill = libc::SIGKILL,
    Segv = libc::SIGSEGV,
    Pipe = libc::SIGPIPE,
    Alarm = libc::SIGALRM,
    Term = libc::SIGTERM,
}

impl Signal {
    #[inline]
    pub fn ignore(self) {
        unsafe {
            state::ignore_signal(self);
        }
    }
}

pub struct ExceptionHandler {
    inner: std::sync::Arc<state::HandlerInner>,
}

impl ExceptionHandler {
    /// Attaches a signal handler. The provided callback will be invoked if a
    /// signal is caught, providing a [`CrashContext`] with the details of
    /// the thread where the signal was thrown.
    ///
    /// The callback runs in a compromised context, so it is highly recommended
    /// to not perform actions that may fail due to corrupted state that caused
    /// or is a symptom of the original signal. This includes doing heap
    /// allocations from the same allocator as the crashing code.
    pub fn attach(on_crash: Box<dyn CrashEvent>) -> Result<Self, Error> {
        unsafe {
            state::install_sigaltstack()?;
            state::install_handlers();
        }

        let inner = std::sync::Arc::new(state::HandlerInner::new(on_crash));

        {
            let mut handlers = state::HANDLER_STACK.lock();
            handlers.push(std::sync::Arc::downgrade(&inner));
        }

        Ok(Self { inner })
    }

    /// Detaches this handler, removing it from the handler stack. This is done
    /// automatically when this [`ExceptionHandler`] is dropped.
    #[inline]
    pub fn detach(self) {
        self.do_detach();
    }

    /// Performs the actual
    fn do_detach(&self) {
        let mut handlers = state::HANDLER_STACK.lock();

        if let Some(ind) = handlers.iter().position(|handler| {
            handler.upgrade().map_or(false, |handler| {
                std::sync::Arc::ptr_eq(&handler, &self.inner)
            })
        }) {
            handlers.remove(ind);

            if handlers.is_empty() {
                unsafe {
                    state::restore_sigaltstack();
                    state::restore_handlers();
                }
            }
        }
    }

    /// Sends the specified user signal.
    pub fn simulate_signal(&self, signal: Signal) -> bool {
        // Normally this would be an unsafe function, since this unsafe encompasses
        // the entirety of the body, however the user is really not required to
        // uphold any guarantees on their end, so no real need to declare the
        // function itself unsafe.
        unsafe {
            let mut siginfo: libc::signalfd_siginfo = std::mem::zeroed();
            siginfo.ssi_code = state::SI_USER;
            siginfo.ssi_pid = std::process::id();

            let mut context = std::mem::zeroed();
            crash_context::crash_context_getcontext(&mut context);

            self.inner.handle_signal(
                signal as i32,
                &mut *(&mut siginfo as *mut libc::signalfd_siginfo).cast::<libc::siginfo_t>(),
                &mut *(&mut context as *mut crash_context::ucontext_t).cast::<libc::c_void>(),
            )
        }
    }
}

impl Drop for ExceptionHandler {
    fn drop(&mut self) {
        self.do_detach();
    }
}
