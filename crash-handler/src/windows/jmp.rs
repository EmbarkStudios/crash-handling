//! Provides an implementation of [`setjmp`] and [`longjmp`], as unfortunately the
//! implementation in MSVCRT actually unwinds the stack

#![cfg(target_arch = "x86_64")]

// Original code from: https://github.com/Snaipe/BoxFort/blob/master/src/asm/setjmp-x86_64.asm
std::arch::global_asm! {
    ".text",
    ".global ehsetjmp",
    ".align 4",
    ".cfi_startproc",
"ehsetjmp:",
    "mov %rbx, 8(%rcx)",
    "mov %rsp, 16(%rcx)",
    "mov %rbp, 24(%rcx)",
    "mov %rsi, 32(%rcx)",
    "mov %rdi, 40(%rcx)",
    "mov %r12, 48(%rcx)",
    "mov %r13, 56(%rcx)",
    "mov %r14, 64(%rcx)",
    "mov %r15, 72(%rcx)",
    "pop 80(%rcx)", // rip
    "push 80(%rcx)",

    "xor %rax, %rax",
    "ret",
    ".cfi_endproc",
    options(att_syntax)
}

std::arch::global_asm! {
    ".text",
    ".global ehlongjmp",
    ".align 4",
    ".cfi_startproc",
"ehlongjmp:",
    "mov 8(%rcx), %rbx",
    "mov 16(%rcx), %rsp",
    "mov 24(%rcx), %rbp",
    "mov 32(%rcx), %rsi",
    "mov 40(%rcx), %rdi",
    "mov 48(%rcx), %r12",
    "mov 56(%rcx), %r13",
    "mov 64(%rcx), %r14",
    "mov 72(%rcx), %r15",
    "pop %rax",
    "push 80(%rcx)",

    "mov %rdx, %rax", // return value
    "ret",
    ".cfi_endproc",
    options(att_syntax)
}

#[repr(C)]
pub struct JmpBuf {
    __jmp_buf: [u128; 16],
}

#[allow(improper_ctypes)] // u128 is actually ok on x86_64 :)
extern "C" {
    #[link_name = "ehsetjmp"]
    pub fn setjmp(jb: *mut JmpBuf) -> i32;
    #[link_name = "ehlongjmp"]
    pub fn longjmp(jb: *mut JmpBuf, val: i32) -> !;
}
