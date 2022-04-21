pub mod jmp;
mod state;

use crate::Error;

use windows_sys::Win32::Foundation as found;

/// Possible exception codes values for the the `exception_code` field
/// in the crash context.
///
/// This is mainly for testing purposes, and is not exhaustive nor really accurate,
/// as eg. a distinction is made between a divide by zero between integers and
/// floats.
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

pub struct CrashHandler;

#[allow(clippy::unused_self)]
impl CrashHandler {
    /// Attaches the crash handler.
    ///
    /// The provided callback will be invoked if an exception is caught,
    /// providing a [`CrashContext`] with the details of the thread where the
    /// exception was thrown.
    pub fn attach(on_crash: Box<dyn crate::CrashEvent>) -> Result<Self, Error> {
        state::attach(on_crash)?;
        Ok(Self)
    }

    /// Detaches this handler, removing it from the handler stack. This is done
    /// automatically when this [`ExceptionHandler`] is dropped.
    #[inline]
    pub fn detach(self) {
        state::detach();
    }

    // Sends the specified user exception
    #[allow(clippy::unused_self)]
    pub fn simulate_exception(&self, exception_code: Option<i32>) -> crate::CrashEventResult {
        // Normally this would be an unsafe function, since this unsafe encompasses
        // the entirety of the body, however the user is really not required to
        // uphold any guarantees on their end, so no real need to declare the
        // function itself unsafe.
        unsafe {
            let lock = state::HANDLER.lock();
            if let Some(handler) = &*lock {
                let mut exception_record: state::EXCEPTION_RECORD = std::mem::zeroed();
                let mut exception_context = std::mem::MaybeUninit::uninit();

                state::RtlCaptureContext(exception_context.as_mut_ptr());

                let mut exception_context = exception_context.assume_init();

                let exception_ptrs = state::EXCEPTION_POINTERS {
                    ExceptionRecord: &mut exception_record,
                    ContextRecord: &mut exception_context,
                };

                let exception_code =
                    exception_code.unwrap_or(state::STATUS_NONCONTINUABLE_EXCEPTION);
                exception_record.ExceptionCode = exception_code;

                let cc = crash_context::CrashContext {
                    exception_pointers: (&exception_ptrs as *const state::EXCEPTION_POINTERS)
                        .cast(),
                    thread_id: state::GetCurrentThreadId(),
                    exception_code,
                };

                handler.user_handler.on_crash(&cc)
            } else {
                crate::CrashEventResult::Handled(false)
            }
        }
    }
}

impl Drop for CrashHandler {
    fn drop(&mut self) {
        state::detach();
    }
}
