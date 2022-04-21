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
    #[cfg_attr(target_env = "gnu", link_name = "__sigsetjmp")]
    pub fn sigsetjmp(jb: *mut JmpBuf, save_mask: i32) -> i32;
    pub fn siglongjmp(jb: *mut JmpBuf, val: i32) -> !;
}
