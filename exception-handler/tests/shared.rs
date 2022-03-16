pub use exception_handler::{debug_print, Signal};
use parking_lot::{Condvar, Mutex};
use std::{mem, sync::Arc};

#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn handles_signal(signal: Signal, raiser: impl Fn()) {
    // the setjmp crate is outdated and uses a convoluted build script backed
    // by bindgen/clang-sys which are extremely outdated, so we just do them
    // here
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
            let tid = libc::syscall(libc::SYS_gettid) as i32;

            handler = Some(
                exception_handler::ExceptionHandler::attach(exception_handler::make_crash_event(
                    move |cc: &exception_handler::CrashContext| {
                        assert_eq!(cc.siginfo.ssi_signo, signal as u32);
                        assert_eq!(cc.tid, tid);

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
