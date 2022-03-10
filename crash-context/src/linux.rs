#[cfg(feature = "fill-minidump")]
pub(crate) mod fill;
mod getcontext;

pub use getcontext::crash_context_getcontext;

#[repr(C)]
#[derive(Clone)]
pub struct sigset_t {
    #[cfg(target_pointer_width = "32")]
    __val: [u32; 32],
    #[cfg(target_pointer_width = "64")]
    __val: [u64; 16],
}

#[repr(C)]
#[derive(Clone)]
pub struct stack_t {
    pub ss_sp: *mut std::ffi::c_void,
    pub ss_flags: i32,
    pub ss_size: usize,
}

cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        #[repr(C)]
        #[derive(Clone)]
        pub struct ucontext_t {
            pub uc_flags: u64,
            pub uc_link: *mut ucontext_t,
            pub uc_stack: stack_t,
            pub uc_mcontext: mcontext_t,
            pub uc_sigmask: sigset_t,
            __private: [u8; 512],
        }

        #[repr(C)]
        #[derive(Clone)]
        pub struct mcontext_t {
            pub gregs: [i64; 23],
            pub fpregs: *mut fpregset_t,
            __reserved: [u64; 8],
        }

        #[repr(C)]
        #[derive(Clone)]
        pub struct fpregset_t {
            pub cwd: u16,
            pub swd: u16,
            pub ftw: u16,
            pub fop: u16,
            pub rip: u64,
            pub rdp: u64,
            pub mxcsr: u32,
            pub mxcr_mask: u32,
            pub st_space: [u32; 32],
            pub xmm_space: [u32; 64],
            __padding: [u64; 12],
        }
    } else if #[cfg(target_arch = "x86")] {
        #[repr(C)]
        #[derive(Clone)]
        pub struct ucontext_t {
            pub uc_flags: u32,
            pub uc_link: *mut ucontext_t,
            pub uc_stack: stack_t,
            pub uc_mcontext: mcontext_t,
            pub uc_sigmask: sigset_t,
            pub __fpregs_mem: [u32; 28],
        }

        #[repr(C)]
        #[derive(Clone)]
        pub struct mcontext_t {
            pub gregs: [i64; 23],
            pub fpregs: *mut fpregset_t,
            pub oldmask: u32,
            pub cr2: u32,
        }

        #[repr(C)]
        #[derive(Clone)]
        pub struct fpreg_t {
            pub significand: [u16; 4],
            pub exponent: u16,
        }

        #[repr(C)]
        #[derive(Clone)]
        pub struct fpregset_t {
            pub cw: u32,
            pub sw: u32,
            pub tag: u32,
            pub ipoff: u32,
            pub cssel: u32,
            pub dataoff: u32,
            pub datasel: u32,
            pub _st: [fpreg_t; 8],
            pub status: u32,
        }
    }
}

/// The full context for a crash linux/android crash
#[repr(C)]
#[derive(Clone)]
pub struct CrashContext {
    /// Crashing thread context.
    ///
    /// Note that we use [`crate::ucontext_t`] instead of [`libc::ucontext_t`]
    /// as libc's differs between glibc and musl <https://github.com/rust-lang/libc/pull/1646>
    /// even though the ucontext_t received from a signal will be the same
    /// regardless of the libc implementation used as it is only arch specific
    /// not libc specific
    pub context: ucontext_t,
    /// State of floating point registers.
    ///
    /// This isn't part of the user ABI for Linux arm, and is already part
    /// of [`crate::ucontext_t`] in mips
    #[cfg(not(any(target_arch = "mips", target_arch = "arm")))]
    pub float_state: fpregset_t,
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

#[cfg(test)]
mod test {
    // Musl doesn't contain fpregs in libc because reasons https://github.com/rust-lang/libc/pull/1646
    #[cfg(not(target_env = "musl"))]
    #[test]
    fn matches_libc() {
        assert_eq!(
            std::mem::size_of::<libc::ucontext_t>(),
            std::mem::size_of::<super::ucontext_t>()
        );
    }
}
