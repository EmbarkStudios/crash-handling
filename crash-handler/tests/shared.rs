#![allow(unsafe_code)]

pub use ch::debug_print;
use crash_handler as ch;
use parking_lot::{Condvar, Mutex};
use std::sync::Arc;

pub use sadness_generator::SadnessFlavor;

pub fn handles_crash(flavor: SadnessFlavor) {
    let _got_it = Arc::new((Mutex::new(false), Condvar::new()));
    let mut _handler = None;

    unsafe {
        // Set a jump point. The first time we are here we set up the signal
        // handler and raise the signal, the signal handler jumps back to here
        // and then we step over the initial block.
        cfg_if::cfg_if! {
            if #[cfg(any(target_os = "linux", target_os = "android"))] {
                let jmpbuf = Arc::new(Mutex::new(std::mem::MaybeUninit::uninit()));
                let val = ch::jmp::sigsetjmp(jmpbuf.lock().as_mut_ptr(), 1 /* save signal mask */);
            } else if #[cfg(target_os = "windows")] {
                let jmpbuf = Arc::new(Mutex::new(std::mem::MaybeUninit::uninit()));
                let val = ch::jmp::setjmp(jmpbuf.lock().as_mut_ptr());
            } else {
                let val = 0;
            }
        }

        if val == 0 {
            let _got_it_in_handler = _got_it;

            _handler = Some(
                ch::CrashHandler::attach(ch::make_crash_event(move |cc: &ch::CrashContext| {
                    cfg_if::cfg_if! {
                        if #[cfg(any(target_os = "linux", target_os = "android"))] {
                            use ch::Signal;

                            assert_eq!(
                                cc.siginfo.ssi_signo,
                                match flavor {
                                    SadnessFlavor::Abort => Signal::Abort,
                                    SadnessFlavor::Bus => Signal::Bus,
                                    SadnessFlavor::DivideByZero => Signal::Fpe,
                                    SadnessFlavor::Illegal => Signal::Illegal,
                                    SadnessFlavor::Segfault | SadnessFlavor::StackOverflow { .. } =>
                                        Signal::Segv,
                                        SadnessFlavor::Trap => Signal::Trap,
                                } as u32
                            );
                            //assert_eq!(cc.tid, tid);

                            // At least on linux these...aren't set. Which is weird
                            //assert_eq!(cc.siginfo.ssi_pid, std::process::id());
                            //assert_eq!(cc.siginfo.ssi_tid, tid as u32);
                        } else if #[cfg(target_os = "macos")] {
                            use ch::ExceptionType;

                            let exc = cc.exception.expect("we should have an exception");

                            let expected = match flavor {
                                SadnessFlavor::Abort => {
                                    assert_eq!(exc.code, 0x10003); // EXC_SOFT_SIGNAL
                                    assert_eq!(exc.subcode.unwrap(), libc::SIGABRT as i64);

                                    ExceptionType::Software
                                }
                                SadnessFlavor::Bus
                                | SadnessFlavor::Segfault
                                | SadnessFlavor::StackOverflow { .. } => ExceptionType::BadAccess,
                                SadnessFlavor::DivideByZero => ExceptionType::Arithmetic,
                                SadnessFlavor::Illegal => ExceptionType::BadInstruction,
                                SadnessFlavor::Trap => ExceptionType::Breakpoint,
                            };

                            assert_eq!(exc.kind, expected as i32);

                            // Unlike other platforms, the mac handler is run in its
                            // own thread, this means that we can't realistically
                            // do a longjmp since the exception was raised on a
                            // different thread, so instead we just take advantage
                            // of the fact that we only run a single test per executable
                            // in these integration tests and just exit the entire
                            // process
                            #[allow(clippy::exit)]
                            std::process::exit(0);
                        } else if #[cfg(target_os = "windows")] {
                            use ch::ExceptionCode;

                            let ec = match flavor {
                                SadnessFlavor::DivideByZero => ExceptionCode::Fpe,
                                SadnessFlavor::Illegal => ExceptionCode::Illegal,
                                SadnessFlavor::InvalidParameter => ExceptionCode::InvalidParameter,
                                SadnessFlavor::Purecall => ExceptionCode::Purecall,
                                SadnessFlavor::Segfault => ExceptionCode::Segv,
                                SadnessFlavor::StackOverflow { .. }=> ExceptionCode::StackOverflow,
                                SadnessFlavor::Trap => ExceptionCode::Trap,
                            };

                            assert_eq!(
                                cc.exception_code, ec as i32,
                                "0x{:x} != 0x{:x}",
                                cc.exception_code, ec as i32
                            );
                        }
                    }

                    #[cfg(not(target_os = "macos"))]
                    {
                        debug_print!("handling signal");
                        {
                            let (lock, cvar) = &*_got_it_in_handler;
                            let mut handled = lock.lock();
                            *handled = true;
                            cvar.notify_one();
                        }

                        // long jump back to before we crashed
                        ch::CrashEventResult::Jump {
                            jmp_buf: jmpbuf.lock().as_mut_ptr(),
                            value: 1,
                        }
                    }
                }))
                .unwrap(),
            );

            flavor.make_sad();
        } else {
            loop {
                std::thread::yield_now();

                let (lock, _cvar) = &*_got_it;
                let signaled = lock.lock();
                if *signaled {
                    debug_print!("signal handled");
                    break;
                }
            }
        }
    }
}
