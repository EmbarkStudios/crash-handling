use crate::Error;
use std::{mem, ptr};

const MIN_STACK_SIZE: usize = 16 * 1024;
/// kill
pub(crate) const SI_USER: i32 = 0;
/// tkill, tgkill
const SI_TKILL: i32 = -6;

struct StackSave {
    old: Option<libc::stack_t>,
    new: libc::stack_t,
}

unsafe impl Send for StackSave {}

static STACK_SAVE: parking_lot::Mutex<Option<StackSave>> = parking_lot::const_mutex(None);

/// Create an alternative stack to run the signal handlers on. This is done since
/// the signal might have been caused by a stack overflow.
pub unsafe fn install_sigaltstack() -> Result<(), Error> {
    // Check to see if the existing sigaltstack, and if it exists, is it big
    // enough. If so we don't need to allocate our own.
    let mut old_stack = mem::zeroed();
    let r = libc::sigaltstack(ptr::null(), &mut old_stack);
    assert_eq!(
        r,
        0,
        "learning about sigaltstack failed: {}",
        std::io::Error::last_os_error()
    );

    if old_stack.ss_flags & libc::SS_DISABLE == 0 && old_stack.ss_size >= MIN_STACK_SIZE {
        return Ok(());
    }

    // ... but failing that we need to allocate our own, so do all that
    // here.
    let page_size = libc::sysconf(libc::_SC_PAGESIZE) as usize;
    let guard_size = page_size;
    let alloc_size = guard_size + MIN_STACK_SIZE;

    let ptr = libc::mmap(
        ptr::null_mut(),
        alloc_size,
        libc::PROT_NONE,
        libc::MAP_PRIVATE | libc::MAP_ANON,
        -1,
        0,
    );
    if ptr == libc::MAP_FAILED {
        return Err(Error::OutOfMemory);
    }

    // Prepare the stack with readable/writable memory and then register it
    // with `sigaltstack`.
    let stack_ptr = (ptr as usize + guard_size) as *mut libc::c_void;
    let r = libc::mprotect(
        stack_ptr,
        MIN_STACK_SIZE,
        libc::PROT_READ | libc::PROT_WRITE,
    );
    assert_eq!(
        r,
        0,
        "mprotect to configure memory for sigaltstack failed: {}",
        std::io::Error::last_os_error()
    );
    let new_stack = libc::stack_t {
        ss_sp: stack_ptr,
        ss_flags: 0,
        ss_size: MIN_STACK_SIZE,
    };
    let r = libc::sigaltstack(&new_stack, ptr::null_mut());
    assert_eq!(
        r,
        0,
        "registering new sigaltstack failed: {}",
        std::io::Error::last_os_error()
    );

    *STACK_SAVE.lock() = Some(StackSave {
        old: (old_stack.ss_flags & libc::SS_DISABLE != 0).then(|| old_stack),
        new: new_stack,
    });

    Ok(())
}

pub unsafe fn restore_sigaltstack() {
    let mut ssl = STACK_SAVE.lock();

    // Only restore the old_stack if the current alternative stack is the one
    // installed by the call to install_sigaltstack.
    if let Some(ss) = &mut *ssl {
        let mut current_stack = mem::zeroed();
        if libc::sigaltstack(ptr::null(), &mut current_stack) == -1 {
            return;
        }

        if current_stack.ss_sp == ss.new.ss_sp {
            if let Some(old) = ss.old {
                // Restore the old alt stack if there was one
                if libc::sigaltstack(&old, ptr::null_mut()) == -1 {
                    return;
                }
            } else {
                // Restore to the default alt stack otherwise
                let mut disable: libc::stack_t = mem::zeroed();
                disable.ss_flags = libc::SS_DISABLE;
                if libc::sigaltstack(&disable, ptr::null_mut()) == -1 {
                    return;
                }
            }
        }

        let r = libc::munmap(ss.new.ss_sp, ss.new.ss_size);
        debug_assert_eq!(r, 0, "munmap failed during thread shutdown");
        *ssl = None;
    }
}

/// Restores the signal handler for the specified signal back to its original
/// handler
unsafe fn install_default_handler(sig: libc::c_int) {
    // Android L+ expose signal and sigaction symbols that override the system
    // ones. There is a bug in these functions where a request to set the handler
    // to SIG_DFL is ignored. In that case, an infinite loop is entered as the
    // signal is repeatedly sent to breakpad's signal handler.
    // To work around this, directly call the system's sigaction.

    cfg_if::cfg_if! {
        if #[cfg(target_os = "android")] {
            let mut sa: libc::sigaction = mem::zeroed();
            libc::sigemptyset(&mut sa.sa_mask);
            sa.sa_sigaction = libc::SIG_DFL;
            sa.sa_flags = libc::SA_RESTART;
            libc::syscall(
                libc::SYS_rt_sigaction,
                sig,
                &sa,
                ptr::null::<libc::sigaction>(),
                mem::size_of::<libc::sigset_t>(),
            );
        } else {
            libc::signal(sig, libc::SIG_DFL);
        }
    }
}

/// The various signals we attempt to handle
const EXCEPTION_SIGNALS: [libc::c_int; 6] = [
    libc::SIGSEGV,
    libc::SIGABRT,
    libc::SIGFPE,
    libc::SIGILL,
    libc::SIGBUS,
    libc::SIGTRAP,
];

static OLD_HANDLERS: parking_lot::Mutex<Option<[libc::sigaction; 6]>> =
    parking_lot::const_mutex(None);

/// Restores all of the signal handlers back to their previous values, or the
/// default if the previous value cannot be restored
pub unsafe fn restore_handlers() {
    let mut ohl = OLD_HANDLERS.lock();

    if let Some(old) = &*ohl {
        for (sig, action) in EXCEPTION_SIGNALS.into_iter().zip(old.iter()) {
            if libc::sigaction(sig, action, ptr::null_mut()) == -1 {
                install_default_handler(sig);
            }
        }
    }

    *ohl = None;
}

pub unsafe fn install_handlers() {
    let mut ohl = OLD_HANDLERS.lock();

    if ohl.is_some() {
        return;
    }

    // Attempt store all of the current handlers so we can restore them later
    let mut old_handlers: [mem::MaybeUninit<libc::sigaction>; 6] =
        mem::MaybeUninit::uninit().assume_init();

    for (sig, handler) in EXCEPTION_SIGNALS
        .iter()
        .copied()
        .zip(old_handlers.iter_mut())
    {
        let mut old = mem::zeroed();
        if libc::sigaction(sig, ptr::null(), &mut old) == -1 {
            return;
        }
        *handler = mem::MaybeUninit::new(old);
    }

    let mut sa: libc::sigaction = mem::zeroed();
    libc::sigemptyset(&mut sa.sa_mask);

    // Mask all exception signals when we're handling one of them.
    for sig in EXCEPTION_SIGNALS {
        libc::sigaddset(&mut sa.sa_mask, sig);
    }

    sa.sa_sigaction = signal_handler as usize;
    sa.sa_flags = libc::SA_ONSTACK | libc::SA_SIGINFO;

    // Use our signal_handler for all of the signals we wish to catch
    for sig in EXCEPTION_SIGNALS {
        // At this point it is impractical to back out changes, and so failure to
        // install a signal is intentionally ignored.
        libc::sigaction(sig, &sa, ptr::null_mut());
    }

    // Everything is initialized. Transmute the array to the
    // initialized type.
    *ohl = Some(mem::transmute::<_, [libc::sigaction; 6]>(old_handlers));
}

pub(crate) static HANDLER_STACK: parking_lot::Mutex<Vec<std::sync::Weak<HandlerInner>>> =
    parking_lot::const_mutex(Vec::new());

unsafe extern "C" fn signal_handler(
    sig: libc::c_int,
    info: *mut libc::siginfo_t,
    uc: *mut libc::c_void,
) {
    let info = &mut *info;
    let uc = &mut *uc;

    {
        let handlers = HANDLER_STACK.lock();

        // Sometimes, Breakpad runs inside a process where some other buggy code
        // saves and restores signal handlers temporarily with 'signal'
        // instead of 'sigaction'. This loses the SA_SIGINFO flag associated
        // with this function. As a consequence, the values of 'info' and 'uc'
        // become totally bogus, generally inducing a crash.
        //
        // The following code tries to detect this case. When it does, it
        // resets the signal handlers with sigaction + SA_SIGINFO and returns.
        // This forces the signal to be thrown again, but this time the kernel
        // will call the function with the right arguments.
        {
            let mut cur_handler = mem::zeroed();
            if libc::sigaction(sig, ptr::null_mut(), &mut cur_handler) == 0
                && cur_handler.sa_sigaction == signal_handler as usize
                && cur_handler.sa_flags & libc::SA_SIGINFO == 0
            {
                // Reset signal handler with the correct flags.
                libc::sigemptyset(&mut cur_handler.sa_mask);
                libc::sigaddset(&mut cur_handler.sa_mask, sig);

                cur_handler.sa_sigaction = signal_handler as usize;
                cur_handler.sa_flags = libc::SA_ONSTACK | libc::SA_SIGINFO;

                if libc::sigaction(sig, &cur_handler, ptr::null_mut()) == -1 {
                    // When resetting the handler fails, try to reset the
                    // default one to avoid an infinite loop here.
                    install_default_handler(sig);
                }

                // exit the handler as we should be called again soon
                return;
            }
        }

        let handled = (|| {
            for handler in handlers.iter() {
                if let Some(handler) = handler.upgrade() {
                    if handler.handle_signal(sig, info, uc) {
                        return true;
                    }
                }
            }

            false
        })();

        // Upon returning from this signal handler, sig will become unmasked and then
        // it will be retriggered. If one of the ExceptionHandlers handled it
        // successfully, restore the default handler. Otherwise, restore the
        // previously installed handler. Then, when the signal is retriggered, it will
        // be delivered to the appropriate handler.
        if handled {
            install_default_handler(sig);
        } else {
            restore_handlers();
        }
    }

    if info.si_code <= 0 || sig == libc::SIGABRT {
        // This signal was triggered by somebody sending us the signal with kill().
        // In order to retrigger it, we have to queue a new signal by calling
        // kill() ourselves.  The special case (si_pid == 0 && sig == SIGABRT) is
        // due to the kernel sending a SIGABRT from a user request via SysRQ.
        let tid = libc::gettid();
        if libc::syscall(libc::SYS_tgkill, std::process::id(), tid, sig) < 0 {
            // If we failed to kill ourselves (e.g. because a sandbox disallows us
            // to do so), we instead resort to terminating our process. This will
            // result in an incorrect exit code.
            libc::_exit(1);
        }
    } else {
        // This was a synchronous signal triggered by a hard fault (e.g. SIGSEGV).
        // No need to reissue the signal. It will automatically trigger again,
        // when we return from the signal handler.
    }
}

/// The size of `CrashContext` can be too big w.r.t the size of alternatate stack
/// for `signal_handler`. Keep the crash context as a .bss field.
static CRASH_CONTEXT: parking_lot::Mutex<mem::MaybeUninit<super::CrashContext>> =
    parking_lot::const_mutex(mem::MaybeUninit::uninit());

pub(crate) struct HandlerInner {
    handler: Box<dyn super::CrashEvent>,
}

impl HandlerInner {
    #[inline]
    pub(crate) fn new(handler: Box<dyn super::CrashEvent>) -> Self {
        Self { handler }
    }

    pub(crate) unsafe fn handle_signal(
        &self,
        _sig: libc::c_int,
        info: &mut libc::siginfo_t,
        uc: &mut libc::c_void,
    ) -> bool {
        // The siginfo_t in libc is lowest common denominator, but this code is
        // specifically targeting linux/android, which contains the si_pid field
        // that we require
        let nix_info = &*((info as *const libc::siginfo_t).cast::<nix::sys::signalfd::siginfo>());

        // Allow ourselves to be dumped if the signal is trusted.
        if info.si_code > 0
            || ((info.si_code == SI_USER || info.si_code == SI_TKILL)
                && nix_info.ssi_pid == std::process::id())
        {
            libc::syscall(libc::SYS_prctl, libc::PR_SET_DUMPABLE, 1, 0, 0, 0);
        }

        let mut crash_ctx = CRASH_CONTEXT.lock();

        {
            *crash_ctx = mem::MaybeUninit::zeroed();

            let mut cc = &mut *crash_ctx.as_mut_ptr();

            ptr::copy_nonoverlapping(nix_info, &mut cc.siginfo, 1);

            let uc_ptr = &*(uc as *const libc::c_void).cast::<uctx::ucontext_t>();
            ptr::copy_nonoverlapping(uc_ptr, &mut cc.context, 1);

            cfg_if::cfg_if! {
                if #[cfg(target_arch = "aarch64")] {
                    let fp_ptr = uc_ptr.uc_mcontext.__reserved.cast::<libc::fpsimd_context>();

                    if fp_ptr.head.magic == libc::FPSIMD_MAGIC {
                        ptr::copy_nonoverlapping(fp_ptr, &mut cc.float_state, mem::size_of::<libc::_libc_fpstate>());
                    }
                } else if #[cfg(not(all(
                    target_arch = "arm",
                    target_arch = "mips",
                    target_arch = "mips64")))] {
                    if !uc_ptr.uc_mcontext.fpregs.is_null() {
                        ptr::copy_nonoverlapping(uc_ptr.uc_mcontext.fpregs, ((&mut cc.float_state) as *mut uctx::fpregset_t).cast(), 1);

                    }
                } else {
                }
            }

            cc.tid = libc::syscall(libc::SYS_gettid) as i32;
        }

        self.handler.on_crash(&*crash_ctx.as_ptr())
    }
}
