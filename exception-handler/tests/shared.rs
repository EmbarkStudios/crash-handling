pub use exception_handler::debug_print;
#[cfg(unix)]
pub use exception_handler::Signal;
use parking_lot::{Condvar, Mutex};
use std::{mem, sync::Arc};

#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn handles_signal(signal: Signal, raiser: impl Fn()) {
    // the setjmp crate uses a convoluted build script backed by bindgen/clang-sys
    // which are extremely outdated, so we just do them here, and libc just doesn't
    // support them all since they're kind of terrible...but useful
    #[repr(C)]
    struct JmpBuf {
        __jmp_buf: [i32; 1],
        __fl: u32,
        __ss: [u32; 32],
    }

    extern "C" {
        #[cfg_attr(target_env = "gnu", link_name = "__sigsetjmp")]
        fn sigsetjmp(jb: *mut JmpBuf, save_mask: i32) -> i32;
        fn siglongjmp(jb: *mut JmpBuf, val: i32) -> !;
    }

    let got_it = Arc::new((Mutex::new(false), Condvar::new()));
    let mut handler = None;

    unsafe {
        let jmpbuf = Arc::new(Mutex::new(mem::MaybeUninit::uninit()));

        // Set a jump point. The first time we are here we set up the signal
        // handler and raise the signal, the signal handler jumps back to here
        // and then we step over the initial block.
        let val = sigsetjmp(jmpbuf.lock().as_mut_ptr(), 1);

        if val == 0 {
            let got_it_in_handler = got_it;
            //let tid = libc::syscall(libc::SYS_gettid) as i32;

            handler = Some(
                exception_handler::ExceptionHandler::attach(exception_handler::make_crash_event(
                    move |cc: &exception_handler::CrashContext| {
                        assert_eq!(cc.siginfo.ssi_signo, signal as u32);
                        //assert_eq!(cc.tid, tid);

                        // At least on linux these...aren't set. Which is weird
                        //assert_eq!(cc.siginfo.ssi_pid, std::process::id());
                        //assert_eq!(cc.siginfo.ssi_tid, tid as u32);

                        debug_print!("handling signal");
                        {
                            let (lock, cvar) = &*got_it_in_handler;
                            let mut handled = lock.lock();
                            *handled = true;
                            cvar.notify_one();
                        }

                        // long jump back to before we crashed
                        siglongjmp(jmpbuf.lock().as_mut_ptr(), 1);

                        //true
                    },
                ))
                .unwrap(),
            );

            raiser();
        } else {
            loop {
                std::thread::yield_now();

                let (lock, _cvar) = &*got_it;
                let signaled = lock.lock();
                if *signaled {
                    debug_print!("signal handled");
                    break;
                }
            }
        }
    }

    // We can't actually clean up the handler since we long jump out of the signal
    // handler, which leaves mutexes still locked since the stack is not unwound
    // so if we don't just forget the hander we'll block infinitely waiting
    // on mutex locks that will never be acquired
    mem::forget(handler);
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod windows {
    use super::*;
    pub use exception_handler::ExceptionCode;

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

    pub fn handles_exception(ec: ExceptionCode, raiser: impl Fn()) {
        /// Not available in libc for obvious reasons, definitions in setjmp.h
        #[repr(C)]
        struct JmpBuf {
            __jmp_buf: [u128; 16],
        }

        // Note that we use our own set/longjmp functions here because the
        // MSVCRT versions actually unwind the stack :p
        #[allow(improper_ctypes)] // u128 is actually ok on x86_64 :)
        extern "C" {
            fn ehsetjmp(jb: *mut JmpBuf) -> i32;
            fn ehlongjmp(jb: *mut JmpBuf, val: i32) -> !;
        }

        let got_it = Arc::new((Mutex::new(false), Condvar::new()));
        let mut handler = None;

        unsafe {
            let jmpbuf = Arc::new(Mutex::new(mem::MaybeUninit::uninit()));

            // Set a jump point. The first time we are here we set up the signal
            // handler and raise the signal, the signal handler jumps back to here
            // and then we step over the initial block.
            let val = ehsetjmp(jmpbuf.lock().as_mut_ptr());

            if val == 0 {
                let got_it_in_handler = got_it;

                handler = Some(
                    exception_handler::ExceptionHandler::attach(
                        exception_handler::make_crash_event(
                            move |cc: &exception_handler::CrashContext| {
                                assert_eq!(
                                    cc.exception_code, ec as i32,
                                    "0x{:x} != 0x{:x}",
                                    cc.exception_code, ec as i32
                                );

                                debug_print!("handling signal");
                                {
                                    let (lock, cvar) = &*got_it_in_handler;
                                    let mut handled = lock.lock();
                                    *handled = true;
                                    cvar.notify_one();
                                }

                                // long jump back to before we crashed
                                debug_print!("long jumping");
                                ehlongjmp(jmpbuf.lock().as_mut_ptr(), 1);

                                //true
                            },
                        ),
                    )
                    .unwrap(),
                );

                raiser();
            } else {
                loop {
                    std::thread::yield_now();

                    let (lock, _cvar) = &*got_it;
                    let signaled = lock.lock();
                    if *signaled {
                        debug_print!("signal handled");
                        break;
                    }
                }
            }
        }

        // We can't actually clean up the handler since we long jump out of the signal
        // handler, which leaves mutexes still locked since the stack is not unwound
        // so if we don't just forget the hander we'll block infinitely waiting
        // on mutex locks that will never be acquired
        mem::forget(handler);
    }
}

#[cfg(target_os = "windows")]
pub use windows::*;
