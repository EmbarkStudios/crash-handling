cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        /// * rflags, rip, rbp, rsp, rbx, r12, r13, r14, r15... these are 8 bytes each
        /// * mxcsr, fp control word, sigmask... these are 4 bytes each
        /// * add 16 ints for future expansion needs...
        /// * and one more since we use sigset/longjmp, to indicate if the mask was saved
        #[repr(C)]
        pub struct JmpBuf([i32; 9 * 2 + 3 + 16 + 1]);
    } else if #[cfg(target_arch = "aarch64")] {
        /// * r21-r29, sp, fp, lr == 12 registers, 8 bytes each. d8-d15
        /// * are another 8 registers, each 8 bytes long. (aapcs64 specifies
        /// * that only 64-bit versions of FP registers need to be saved).
        /// * Finally, two 8-byte fields for signal handling purposes.
        /// * and one more since we use sigset/longjmp, to indicate if the mask was saved
        #[repr(C)]
        pub struct JmpBuf([i32; (14 + 8 + 2) * 2 + 1]);
    }
}

extern "C" {
    pub fn sigsetjmp(jb: *mut JmpBuf, save_mask: i32) -> i32;
    pub fn siglongjmp(jb: *mut JmpBuf, val: i32) -> !;
}
