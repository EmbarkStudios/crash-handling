mod state;

use crate::Error;

use windows_sys::Win32::Foundation as found;

/// Possible exception codes values for the the `exception_code` field
/// in the crash context.
///
/// This is mainly for testing purposes, and is not exhaustive nor really accurate,
/// as eg. a distinction is made between a floating point divide by zero between
/// integers and floats.
#[derive(Copy, Clone)]
#[repr(i32)]
pub enum ExceptionCode {
    Fpe = found::EXCEPTION_INT_DIVIDE_BY_ZERO,
    Illegal = found::EXCEPTION_ILLEGAL_INSTRUCTION,
    Segv = found::EXCEPTION_ACCESS_VIOLATION,
    StackOverflow = found::EXCEPTION_STACK_OVERFLOW,
    Trap = found::EXCEPTION_BREAKPOINT,
    InvalidParam = found::STATUS_INVALID_PARAMETER,
    Purecall = found::STATUS_NONCONTINUABLE_EXCEPTION,
}

pub struct ExceptionHandler {
    inner: std::sync::Arc<state::HandlerInner>,
}

#[derive(Copy, Clone, PartialEq)]
pub enum HandleDebugExceptions {
    Yes,
    No,
}

impl From<HandleDebugExceptions> for bool {
    fn from(hde: HandleDebugExceptions) -> bool {
        hde == HandleDebugExceptions::Yes
    }
}

impl ExceptionHandler {
    /// Attaches an exception handler.
    ///
    /// The provided callback will be invoked if an exception is caught,
    /// providing a [`CrashContext`] with the details of the thread where the
    /// exception was thrown.
    ///
    /// The callback runs in a compromised context, so it is highly recommended
    /// to not perform actions that may fail due to corrupted state that caused
    /// or is a symptom of the original exception. This includes doing heap
    /// allocations from the same allocator as the crashing code.
    pub fn attach(on_crash: Box<dyn crate::CrashEvent>) -> Result<Self, Error> {
        let inner = {
            let mut handlers = state::HANDLER_STACK.lock();
            let inner = std::sync::Arc::new(state::HandlerInner::new(
                HandleDebugExceptions::Yes,
                on_crash,
            ));
            handlers.push(std::sync::Arc::downgrade(&inner));
            inner
        };

        Ok(Self { inner })
    }

    /// Detaches this handler, removing it from the handler stack. This is done
    /// automatically when this [`ExceptionHandler`] is dropped.
    #[inline]
    pub fn detach(self) {
        self.do_detach();
    }

    /// Performs the actual handler deregistration
    fn do_detach(&self) {
        let mut handlers = state::HANDLER_STACK.lock();

        if let Some(ind) = handlers.iter().position(|handler| {
            handler.upgrade().map_or(false, |handler| {
                std::sync::Arc::ptr_eq(&handler, &self.inner)
            })
        }) {
            let removed = handlers.remove(ind);

            // Breakpad prints a warning if you remove a handler in the middle
            // of the stack, but this seems better
            if handlers.last().is_some() {
                state::set_handlers();
            } else if let Some(removed) = removed.upgrade() {
                state::set_previous_handlers(removed);
            }
        }
    }

    // Sends the specified user signal.
    // pub fn simulate_signal(&self, signal: Signal) -> bool {
    //     // Normally this would be an unsafe function, since this unsafe encompasses
    //     // the entirety of the body, however the user is really not required to
    //     // uphold any guarantees on their end, so no real need to declare the
    //     // function itself unsafe.
    //     unsafe {
    //         let mut siginfo: libc::signalfd_siginfo = std::mem::zeroed();
    //         siginfo.ssi_code = state::SI_USER;
    //         siginfo.ssi_pid = std::process::id();

    //         let mut context = std::mem::zeroed();
    //         crash_context::crash_context_getcontext(&mut context);

    //         self.inner.handle_signal(
    //             signal as i32,
    //             &mut *(&mut siginfo as *mut libc::signalfd_siginfo).cast::<libc::siginfo_t>(),
    //             &mut *(&mut context as *mut crash_context::ucontext_t).cast::<libc::c_void>(),
    //         )
    //     }
    // }
}

impl Drop for ExceptionHandler {
    fn drop(&mut self) {
        self.do_detach();
    }
}
