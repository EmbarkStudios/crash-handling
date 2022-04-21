pub mod jmp;
mod state;

use crate::Error;

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

pub struct CrashHandler;

#[allow(clippy::unused_self)]
impl CrashHandler {
    /// Attaches the signal handler.
    ///
    /// The provided callback will be invoked if a signal is caught, providing a
    /// [`CrashContext`] with the details of the thread where the signal was raised.
    ///
    /// The callback runs in a compromised context, so it is highly recommended
    /// to not perform actions that may fail due to corrupted state that caused
    /// or is a symptom of the original signal. This includes doing heap
    /// allocations from the same allocator as the crashing code.
    pub fn attach(on_crash: Box<dyn crate::CrashEvent>) -> Result<Self, Error> {
        state::attach(on_crash)?;
        Ok(Self)
    }

    /// Detaches the handler.
    ///
    /// This is done automatically when this [`CrashHandler`] is dropped.
    #[inline]
    pub fn detach(self) {
        state::detach();
    }

    /// Sends the specified user signal.
    pub fn simulate_signal(&self, signal: Signal) -> crate::CrashEventResult {
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

            let lock = state::HANDLER.lock();
            if let Some(handler) = &*lock {
                handler.handle_signal(
                    signal as i32,
                    &mut *(&mut siginfo as *mut libc::signalfd_siginfo).cast::<libc::siginfo_t>(),
                    &mut *(&mut context as *mut crash_context::ucontext_t).cast::<libc::c_void>(),
                )
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
