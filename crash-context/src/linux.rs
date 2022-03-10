/// The full context for a crash
#[repr(C)]
#[derive(Clone)]
pub struct CrashContext {
    /// Crashing thread context
    pub context: crate::ucontext_t,
    /// Float state. This isn't part of the user ABI for Linux aarch, and is
    /// already part of ucontext_t in mips
    #[cfg(not(any(target_arch = "mips", target_arch = "arm")))]
    pub float_state: crate::fpregset_t,
    /// The signal info for the crash
    pub siginfo: libc::signalfd_siginfo,
    /// The id of the crashing thread
    pub tid: libc::pid_t,
}

unsafe impl Send for CrashContext {}

impl CrashContext {
    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            let size = std::mem::size_of_val(self);
            let ptr = (self as *const Self).cast();
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
