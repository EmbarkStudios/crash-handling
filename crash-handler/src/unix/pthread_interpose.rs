//! Interposes calls to `pthread_create` so that we always install an alternate
//! signal stack.
//!
//! Original code from <https://hg.mozilla.org/mozilla-central/file/3cf2b111807aec49c54bc958771177d33925aace/toolkit/crashreporter/pthread_create_interposer/pthread_create_interposer.cpp>

#![allow(non_camel_case_types)]

use libc::c_void;
use std::ptr;

pub type pthread_main_t = unsafe extern "C" fn(_: *mut c_void) -> *mut c_void;

#[cfg(not(miri))]
type pthread_create_t = unsafe extern "C" fn(
    thread: *mut libc::pthread_t,
    attr: *const libc::pthread_attr_t,
    f: pthread_main_t,
    arg: *mut c_void,
) -> i32;

struct PthreadCreateParams {
    main: pthread_main_t,
    arg: *mut c_void,
}

/// Key created at first thread creation so that we can set the thread specific
/// alternate stack memory as per-thread data that is uninstalled and unmapped
/// in the `pthread_key` destructor
static mut THREAD_DESTRUCTOR_KEY: libc::pthread_key_t = 0;

#[cfg(all(target_env = "musl", target_feature = "crt-static", not(miri)))]
unsafe extern "C" {
    /// This is the weak alias for `pthread_create`. We declare this so we can
    /// use its address when targeting musl, as we can't lookup the actual
    /// `pthread_create` symbol at runtime since we've interposed it.
    pub fn __pthread_create(
        thread: *mut libc::pthread_t,
        attr: *const libc::pthread_attr_t,
        main: pthread_main_t,
        arg: *mut c_void,
    ) -> i32;
}

#[cfg(all(feature = "sanitizer-compat", not(miri)))]
unsafe extern "C" {
    /// A link-time weak reference to a sanitizer's `pthread_create` interceptor.
    ///
    /// A sanitizer (ASan/LSan/TSan/...) exports a *strong*
    /// `__interceptor_pthread_create` plus a *weak* `pthread_create` alias. Our
    /// strong `#[no_mangle] pthread_create` shadows that weak alias, so resolving
    /// the "real" `pthread_create` via `dlsym(RTLD_NEXT, ...)` jumps straight to
    /// libc and bypasses the sanitizer's create-side bookkeeping — while its
    /// `pthread_join` interceptor stays active, desyncing its thread registry and
    /// aborting at teardown (issue #123).
    ///
    /// Declared `extern_weak` so it resolves to the interceptor when a sanitizer
    /// is linked and to `None` (a null address) otherwise, leaving non-sanitizer
    /// builds untouched. When present we chain to it instead of libc, keeping the
    /// sanitizer in the loop — the call becomes `pthread_create` ->
    /// `__interceptor_pthread_create` -> libc. The sanitizer resolves its own real
    /// `pthread_create` via `RTLD_NEXT` from the runtime's position, so there is no
    /// recursion.
    ///
    /// `dlsym(RTLD_DEFAULT, "__interceptor_pthread_create")` is *not* a substitute:
    /// the interceptor is statically linked and absent from the executable's
    /// dynamic symbol table, so `dlsym` returns null.
    #[linkage = "extern_weak"]
    static __interceptor_pthread_create: Option<pthread_create_t>;
}

/// This interposer replaces `pthread_create` so that we can inject an
/// alternate signal stack in every new thread, regardless of whether the
/// thread is created directly in Rust's std library or not
///
/// # Errors
///
/// This will fail if we're unable to retrieve the address of the actual
/// libc `pthread_create`, or if we do find the address but it's actually the
/// address of this interpose function which would result in infinte recursion
#[cfg(not(miri))]
#[unsafe(no_mangle)]
pub extern "C" fn pthread_create(
    thread: *mut libc::pthread_t,
    attr: *const libc::pthread_attr_t,
    main: pthread_main_t,
    arg: *mut c_void,
) -> i32 {
    /// Get the address of the _real_ `pthread_create`
    static mut REAL_PTHREAD_CREATE: Option<pthread_create_t> = None;
    static INIT: parking_lot::Once = parking_lot::Once::new();

    // Finds the real pthread_create and specifies the pthread_key that is
    // used to uninstall and unmap the alternate stack
    INIT.call_once(|| unsafe {
        // Under `sanitizer-compat`, if a sanitizer's interceptor is linked, chain
        // to it rather than resolving the real `pthread_create` directly, so the
        // sanitizer's thread bookkeeping stays in sync (issue #123). The weak
        // reference is `None` when no sanitizer is present, so we fall through to
        // the normal resolution below.
        #[cfg(feature = "sanitizer-compat")]
        let sanitizer_interceptor =
            __interceptor_pthread_create.map(|interceptor| interceptor as usize as *mut c_void);
        #[cfg(not(feature = "sanitizer-compat"))]
        let sanitizer_interceptor: Option<*mut c_void> = None;

        let ptr = if let Some(interceptor) = sanitizer_interceptor {
            interceptor
        } else {
            cfg_if::cfg_if! {
                if #[cfg(all(target_env = "musl", target_feature = "crt-static"))] {
                    __pthread_create as *mut c_void
                } else {
                    const RTLD_NEXT: *mut c_void = -1isize as *mut c_void;
                    libc::dlsym(RTLD_NEXT, c"pthread_create".as_ptr().cast())
                }
            }
        };

        if !ptr.is_null() {
            REAL_PTHREAD_CREATE = Some(std::mem::transmute::<*mut libc::c_void, pthread_create_t>(
                ptr,
            ));
        }

        libc::pthread_key_create(
            std::ptr::addr_of_mut!(THREAD_DESTRUCTOR_KEY),
            Some(uninstall_sig_alt_stack),
        );
    });

    #[allow(static_mut_refs)]
    let real_pthread_create = unsafe {
        let real_pthread_create = REAL_PTHREAD_CREATE.as_ref().expect("pthread_create() intercept failed but the intercept function is still being called, this won't work");
        assert!(
            !std::ptr::fn_addr_eq(*real_pthread_create, pthread_create as pthread_create_t),
            "We could not obtain the real pthread_create(). Calling the symbol we got would make us enter an infinte loop so stop here instead."
        );
        real_pthread_create
    };

    let create_params = Box::new(PthreadCreateParams { main, arg });
    let create_params = Box::into_raw(create_params);

    let result = unsafe {
        real_pthread_create(
            thread,
            attr,
            set_alt_signal_stack_and_start,
            create_params.cast(),
        )
    };

    if result != 0 {
        // Only deallocate if the thread fails to spawn, if it succeeds it
        // will deallocate the box itself
        unsafe {
            drop(Box::from_raw(create_params));
        }
    }

    result
}

// std::cmp::max is not const :(
const fn get_stack_size() -> usize {
    if libc::SIGSTKSZ > 16 * 1024 {
        libc::SIGSTKSZ
    } else {
        16 * 1024
    }
}

/// The size of the alternate stack that is mapped for every thread.
///
/// This has a minimum size of 16k, which might seem a bit large, but this
/// memory will only ever be committed in case we actually get a stack overflow,
/// which is (hopefully) exceedingly rare
const SIG_STACK_SIZE: usize = get_stack_size();

/// This is the replacment function for the user's thread entry, it installs
/// the alternate stack before invoking the original thread entry, then cleans
/// it up after the user's thread entry exits.
#[unsafe(no_mangle)]
unsafe extern "C" fn set_alt_signal_stack_and_start(params: *mut c_void) -> *mut libc::c_void {
    let (user_main, user_arg) = {
        let params = unsafe { Box::from_raw(params.cast::<PthreadCreateParams>()) };

        (params.main, params.arg)
    };

    let alt_stack_mem = unsafe { install_sig_alt_stack() };

    // The original code was using pthread_cleanup_push/pop, however those are
    // macros in glibc/musl, so we instead use pthread_key_create as it works
    // functionally the same and can call a cleanup function/destructor on both
    // thread exit and cancel
    unsafe {
        libc::pthread_setspecific(THREAD_DESTRUCTOR_KEY, alt_stack_mem);
        user_main(user_arg)
    }
}

/// Install the alternate signal stack
///
/// Returns a pointer to the memory area we mapped to store the stack only if it
/// was installed successfully, otherwise returns `null`.
///
/// # Errors
///
/// If we're able to map memory, but unable to install the alternate stack, we
/// expect that we can unmap the memory
unsafe fn install_sig_alt_stack() -> *mut libc::c_void {
    let alt_stack_mem = unsafe {
        libc::mmap(
            ptr::null_mut(),
            SIG_STACK_SIZE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };

    // Check that we successfully mapped some memory
    if alt_stack_mem.is_null() {
        return alt_stack_mem;
    }

    let alt_stack = libc::stack_t {
        ss_sp: alt_stack_mem,
        ss_flags: 0,
        ss_size: SIG_STACK_SIZE,
    };

    // Attempt to install the alternate stack
    let rv = unsafe { libc::sigaltstack(&alt_stack, ptr::null_mut()) };

    // Attempt to cleanup the mapping if we failed to install the alternate stack
    if rv != 0 {
        assert_eq!(
            unsafe { libc::munmap(alt_stack_mem, SIG_STACK_SIZE) },
            0,
            "failed to install an alternate signal stack, and failed to unmap the alternate stack memory"
        );
        ptr::null_mut()
    } else {
        alt_stack_mem
    }
}

/// Uninstall the alternate signal stack and unmaps the memory.
///
/// # Errors
///
/// If the alternate stack is not `null`, it is expected that uninstalling and
/// unmapping will not error
#[unsafe(no_mangle)]
unsafe extern "C" fn uninstall_sig_alt_stack(alt_stack_mem: *mut libc::c_void) {
    if alt_stack_mem.is_null() {
        return;
    }

    let disable_stack = libc::stack_t {
        ss_sp: ptr::null_mut(),
        ss_flags: libc::SS_DISABLE,
        ss_size: 0,
    };

    // Attempt to uninstall the alternate stack
    assert_eq!(
        unsafe { libc::sigaltstack(&disable_stack, ptr::null_mut()) },
        0,
        "failed to uninstall alternate signal stack"
    );
    assert_eq!(
        unsafe { libc::munmap(alt_stack_mem, SIG_STACK_SIZE) },
        0,
        "failed to unmap alternate stack memory"
    );
}
