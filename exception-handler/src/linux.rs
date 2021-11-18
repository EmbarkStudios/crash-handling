mod state;

use crate::Error;

/// The full context for a crash
#[repr(C)]
#[derive(Clone)]
pub struct CrashContext {
    /// Crashing thread context
    pub context: uctx::ucontext_t,
    /// Float state. This isn't part of the user ABI for Linux aarch, and is
    /// already part of ucontext_t in mips
    #[cfg(not(any(target_arch = "mips", target_arch = "arm")))]
    pub float_state: uctx::fpregset_t,
    /// The signal info for the crash
    pub siginfo: libc::signalfd_siginfo,
    /// The id of the crashing thread
    pub tid: libc::pid_t,
}

unsafe impl Send for CrashContext {}

impl CrashContext {
    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            eprintln!("msghdr {}", std::mem::size_of::<libc::msghdr>());
            let size = dbg!(std::mem::size_of_val(self));
            let ptr = self as *const Self as *const u8;
            std::slice::from_raw_parts(ptr, size)
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != std::mem::size_of::<Self>() {
            return None;
        }

        unsafe { Some((*bytes.as_ptr().cast::<Self>()).clone()) }
    }
}

pub trait CrashEvent: Send + Sync {
    /// Method invoked when a crash occurs. Returning true indicates your handler
    /// has processed the crash and that no further handlers should run.
    fn on_crash(&self, context: &CrashContext) -> bool;
}

impl<F> CrashEvent for F
where
    F: Send + Sync + Fn(&CrashContext) -> bool,
{
    fn on_crash(&self, context: &CrashContext) -> bool {
        (self)(context)
    }
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
            uctx::getcontext(&mut context);

            self.inner.handle_signal(
                signal as i32,
                &mut *(&mut siginfo as *mut libc::signalfd_siginfo).cast::<libc::siginfo_t>(),
                &mut *(&mut context as *mut uctx::ucontext_t).cast::<libc::c_void>(),
            )
        }
    }
}

impl Drop for ExceptionHandler {
    fn drop(&mut self) {
        println!("WAIT WTF IS DROP BEING CALLED");
        self.do_detach();
    }
}
