cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        #[repr(C)]
        pub struct __jmp_buf([u64; 8]);
    } else if #[cfg(target_arch = "x86")] {
        #[repr(C)]
        pub struct __jmp_buf([u32; 6]);
    } else if #[cfg(target_arch = "arm")] {
        #[repr(C)]
        pub struct __jmp_buf([u64; 32]);
    } else if #[cfg(target_arch = "aarch64")] {
        #[repr(C)]
        pub struct __jmp_buf([u64; 22]);
    }
}

/// A jump buffer, which is essentially the register state of a point in execution
/// at the time of a [`sigestjmp`] call that can be returned to by passing this
/// buffer to [`siglongjmp`].
#[repr(C)]
pub struct JmpBuf {
    /// CPU context
    __jmp_buf: __jmp_buf,
    /// Whether the signal mask was saved
    __fl: u32,
    /// Saved signal mask
    __ss: [u32; 32],
}

extern "C" {
    /// Set jump point for a non-local goto.
    ///
    /// The return value will be 0 if this is a direct invocation (ie the "first
    /// time" `sigsetjmp` is executed), and will be the value passed to `siglongjmp`
    /// otherwise.
    ///
    /// See [sigsetjmp](https://man7.org/linux/man-pages/man3/sigsetjmp.3p.html)
    /// for more information.
    #[cfg_attr(target_env = "gnu", link_name = "__sigsetjmp")]
    pub fn sigsetjmp(jb: *mut JmpBuf, save_mask: i32) -> i32;
    /// Non-local goto with signal handling
    ///
    /// The value passed here will be returned by `sigsetjmp` when returning
    /// to that callsite. Note that passing a value of 0 here will be changed
    /// to a 1.
    ///
    /// See [siglongjmp](https://man7.org/linux/man-pages/man3/siglongjmp.3p.html)
    /// for more information.
    pub fn siglongjmp(jb: *mut JmpBuf, val: i32) -> !;
}
