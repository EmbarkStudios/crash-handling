pub use eh::debug_print;
use exception_handler as eh;
use parking_lot::{Condvar, Mutex};
use std::{mem, sync::Arc};

#[allow(dead_code)]
pub enum ExceptionKind {
    Abort,
    Bus,
    Fpe,
    Illegal,
    InvalidParam,
    Purecall,
    SigSegv,
    StackOverflow,
    Trap,
}

pub fn handles_exception(signal: ExceptionKind, raiser: impl Fn()) {
    let got_it = Arc::new((Mutex::new(false), Condvar::new()));
    let mut _handler = None;

    unsafe {
        let jmpbuf = Arc::new(Mutex::new(mem::MaybeUninit::uninit()));

        // Set a jump point. The first time we are here we set up the signal
        // handler and raise the signal, the signal handler jumps back to here
        // and then we step over the initial block.
        #[cfg(unix)]
        let val = eh::jmp::sigsetjmp(jmpbuf.lock().as_mut_ptr(), 1 /* save signal mask */);
        #[cfg(windows)]
        let val = eh::jmp::setjmp(jmpbuf.lock().as_mut_ptr());

        if val == 0 {
            let got_it_in_handler = got_it;

            _handler = Some(
                eh::ExceptionHandler::attach(eh::make_crash_event(move |cc: &eh::CrashContext| {
                    #[cfg(any(target_os = "linux", target_os = "android"))]
                    {
                        use eh::Signal;

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
                        use eh::ExceptionType;

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

                        // Unlike other platforms, the mac handler is run in its
                        // own thread, this means that we can't realistically
                        // do a longjmp since the exception was raised on a
                        // different thread, so instead we just take advantage
                        // of the fact that we only run a single test per executable
                        // in these integration tests and just exit the entire
                        // process
                        std::process::exit(0);
                    }

                    #[cfg(target_os = "windows")]
                    {
                        use eh::ExceptionCode;

                        let ec = match signal {
                            ExceptionKind::Abort | ExceptionKind::Bus => unimplemented!(),
                            ExceptionKind::Fpe => ExceptionCode::Fpe,
                            ExceptionKind::Illegal => ExceptionCode::Illegal,
                            ExceptionKind::InvalidParam => ExceptionCode::InvalidParam,
                            ExceptionKind::Purecall => ExceptionCode::Purecall,
                            ExceptionKind::SigSegv => ExceptionCode::Segv,
                            ExceptionKind::StackOverflow => ExceptionCode::StackOverflow,
                            ExceptionKind::Trap => ExceptionCode::Trap,
                        };

                        assert_eq!(
                            cc.exception_code, ec as i32,
                            "0x{:x} != 0x{:x}",
                            cc.exception_code, ec as i32
                        );
                    }

                    debug_print!("handling signal");
                    {
                        let (lock, cvar) = &*got_it_in_handler;
                        let mut handled = lock.lock();
                        *handled = true;
                        cvar.notify_one();
                    }

                    // long jump back to before we crashed
                    eh::CrashEventResult::Jump {
                        jmp_buf: jmpbuf.lock().as_mut_ptr(),
                        value: 1,
                    }
                }))
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
}
