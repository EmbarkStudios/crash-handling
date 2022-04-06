//! libc doesn't include jmp related bindings at all, since they're terrible...
//! but useful, and the setjmp crate uses a convoluted build script backed by
//! (extremely outdated) bindgen/clang-sys just to rename a single function

use super::*;

cfg_if::cfg_if! {
    if #[cfg(any(target_os = "linux", target_os = "android"))] {
        cfg_if::cfg_if! {
            if #[cfg(target_arch = "x86_64")] {
                #[repr(C)]
                struct __jmp_buf([u64; 8]);
            } else if #[cfg(target_arch = "x86")] {
                #[repr(C)]
                struct __jmp_buf([u32; 6]);
            } else if #[cfg(target_arch = "arm")] {
                #[repr(C)]
                struct __jmp_buf([u64; 32]);
            } else if #[cfg(target_arch = "aarch64")] {
                #[repr(C)]
                struct __jmp_buf([u64; 22]);
            }
        }

        #[repr(C)]
        struct JmpBuf {
            /// CPU context
            __jmp_buf: __jmp_buf,
            /// Whether the signal mask was saved
            __fl: u32,
            /// Saved signal mask
            __ss: [u32; 32],
        }
    } else if #[cfg(all(target_os = "macos", target_arch = "x86_64"))] {
        /// * rflags, rip, rbp, rsp, rbx, r12, r13, r14, r15... these are 8 bytes each
        /// * mxcsr, fp control word, sigmask... these are 4 bytes each
        /// * add 16 ints for future expansion needs...
        /// * and one more since we use sigset/longjmp, to indicate if the mask was saved
        #[repr(C)]
        struct JmpBuf([i32; 9 * 2 + 3 + 16 + 1]);
    } else if #[cfg(all(target_os = "macos", target_arch = "aarch64"))] {
        /// * r21-r29, sp, fp, lr == 12 registers, 8 bytes each. d8-d15
        /// * are another 8 registers, each 8 bytes long. (aapcs64 specifies
        /// * that only 64-bit versions of FP registers need to be saved).
        /// * Finally, two 8-byte fields for signal handling purposes.
        /// * and one more since we use sigset/longjmp, to indicate if the mask was saved
        #[repr(C)]
        struct JmpBuf([i32; (14 + 8 + 2) * 2 + 1]);
    }
}

extern "C" {
    #[cfg_attr(target_env = "gnu", link_name = "__sigsetjmp")]
    fn sigsetjmp(jb: *mut JmpBuf, save_mask: i32) -> i32;
    fn siglongjmp(jb: *mut JmpBuf, val: i32) -> !;
}

pub fn handles_exception(signal: ExceptionKind, raiser: impl Fn()) {
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
                        #[cfg(any(target_os = "linux", target_os = "android"))]
                        {
                            use exception_handler::Signal;

                            assert_eq!(
                                cc.siginfo.ssi_signo,
                                match signal {
                                    ExceptionKind::Abort => Signal::Abort,
                                    ExceptionKind::Bus => Signal::Bus,
                                    ExceptionKind::Fpe => Signal::Fpe,
                                    ExceptionKind::Illegal => Signal::Ill,
                                    ExceptionKind::SigSegv | ExceptionKind::StackOverflow =>
                                        Signal::Segv,
                                    ExceptionKind::Trap => Signal::Trap,
                                    ExceptionKind::InvalidParam | ExceptionKind::Purecall =>
                                        unreachable!(),
                                } as u32
                            );
                            //assert_eq!(cc.tid, tid);

                            // At least on linux these...aren't set. Which is weird
                            //assert_eq!(cc.siginfo.ssi_pid, std::process::id());
                            //assert_eq!(cc.siginfo.ssi_tid, tid as u32);
                        }

                        #[cfg(target_os = "macos")]
                        {
                            use exception_handler::ExceptionType;

                            let exc = cc.exception.expect("we should have an exception");

                            let expected = match signal {
                                ExceptionKind::Abort => {
                                    assert_eq!(exc.code, 0x10003); // EXC_SOFT_SIGNAL
                                    assert_eq!(exc.subcode.unwrap(), libc::SIGABRT as i64);

                                    ExceptionType::Software
                                }
                                ExceptionKind::Bus
                                | ExceptionKind::SigSegv
                                | ExceptionKind::StackOverflow => ExceptionType::BadAccess,
                                ExceptionKind::Fpe => ExceptionType::Arithmetic,
                                ExceptionKind::Illegal => ExceptionType::BadInstruction,
                                ExceptionKind::Trap => ExceptionType::Breakpoint,
                                ExceptionKind::InvalidParam | ExceptionKind::Purecall => {
                                    unreachable!()
                                }
                            };

                            assert_eq!(exc.kind, expected as i32);
                        }

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
