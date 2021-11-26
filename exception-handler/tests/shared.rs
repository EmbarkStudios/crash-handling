pub use exception_handler::Signal;

use std::{
    mem::MaybeUninit,
    sync::{self as ss, atomic},
};

#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn handles_signal(signal: Signal, raiser: impl Fn()) {
    let got_it = ss::Arc::new(atomic::AtomicBool::new(false));
    let mut handler = None;

    unsafe {
        let jmpbuf = ss::Arc::new(parking_lot::Mutex::new(MaybeUninit::uninit()));

        // Set a jump point. The first time we are here we set up the signal
        // handler and raise the signal, the signal handler jumps back to here
        // and then we step over the initial block.
        let val = setjmp::sigsetjmp(jmpbuf.lock().as_mut_ptr(), 1);

        if val == 0 {
            let got_it_in_handler = got_it.clone();
            let tid = libc::syscall(libc::SYS_gettid) as i32;

            handler = Some(
                exception_handler::ExceptionHandler::attach(exception_handler::make_crash_event(
                    move |cc: &exception_handler::CrashContext| {
                        assert_eq!(cc.siginfo.ssi_signo, signal as u32);
                        assert_eq!(cc.tid, tid);

                        // At least on linux these...aren't set. Which is weird
                        //assert_eq!(cc.siginfo.ssi_pid, std::process::id());
                        //assert_eq!(cc.siginfo.ssi_tid, tid as u32);

                        got_it_in_handler.store(true, atomic::Ordering::Relaxed);

                        // long jump back to before we crashed
                        setjmp::siglongjmp(jmpbuf.lock().as_mut_ptr(), 1);

                        //true
                    },
                ))
                .unwrap(),
            );

            raiser();
        }

        assert!(got_it.load(atomic::Ordering::Relaxed));
    }

    // We can't actually clean up the handler since we long jump out of the signal
    // handler, which leaves mutexes still locked since the stack is not unwound
    // so if we don't just forget the hander we'll block infinitely waiting
    // on mutex locks that will never be acquired
    std::mem::forget(handler);
}
