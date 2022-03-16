//! Intercepts calls to `pthread_create` so that we always install an alternate
//! signal stack.
//!
//! Original code from <https://hg.mozilla.org/mozilla-central/file/3cf2b111807aec49c54bc958771177d33925aace/toolkit/crashreporter/pthread_create_interposer/pthread_create_interposer.cpp>

#![allow(non_camel_case_types)]

use libc::c_void;
use std::ptr;

pub type pthread_main_t = unsafe extern "C" fn(_: *mut c_void) -> *mut c_void;
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

static mut THREAD_DESTRUCTOR_KEY: libc::pthread_key_t = 0;

#[cfg(target_env = "musl")]
extern "C" {
    pub fn __pthread_create(
        thread: *mut libc::pthread_t,
        attr: *const libc::pthread_attr_t,
        main: pthread_main_t,
        arg: *mut c_void,
    ) -> i32;
}

/// This interposer replaces `pthread_create` so that we can inject an
/// alternate signal stack in every new thread, regardless of whether the
/// thread is created directly in Rust's std library or not
#[no_mangle]
pub extern "C" fn pthread_create(
    thread: *mut libc::pthread_t,
    attr: *const libc::pthread_attr_t,
    main: pthread_main_t,
    arg: *mut c_void,
) -> i32 {
    /// Get the address of the _real_ `pthread_create`
    static mut REAL_PTHREAD_CREATE: Option<pthread_create_t> = None;
    static INIT: parking_lot::Once = parking_lot::Once::new();

    INIT.call_once(|| unsafe {
        let ptr;

        #[cfg(target_env = "musl")]
        {
            ptr = __pthread_create as *const c_void;
        }
        #[cfg(not(target_env = "musl"))]
        {
            ptr = libc::dlsym(libc::RTLD_NEXT, b"pthread_create\0".as_ptr().cast());
        }

        if !ptr.is_null() {
            REAL_PTHREAD_CREATE = Some(std::mem::transmute(ptr));
        }

        libc::pthread_key_create(&mut THREAD_DESTRUCTOR_KEY, Some(uninstall_sig_alt_stack));
    });

    let real_pthread_create = unsafe { REAL_PTHREAD_CREATE.as_ref() }.expect("pthread_create() intercept failed but the intercept function is still being called, this won't work");
    assert!(*real_pthread_create != pthread_create as pthread_create_t, "We could not obtain the real pthread_create(). Calling the symbol we got would make us enter an infinte loop so stop here instead.");

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

const SIG_STACK_SIZE: usize = get_stack_size();

/// This is the replacment function for the user's thread entry, it installs
/// the alternate stack before invoking the original thread entry, then cleans
/// it up after the user's thread entry exits.
#[no_mangle]
unsafe extern "C" fn set_alt_signal_stack_and_start(params: *mut c_void) -> *mut libc::c_void {
    let (user_main, user_arg) = {
        let params = Box::from_raw(params.cast::<PthreadCreateParams>());

        (params.main, params.arg)
    };

    let alt_stack_mem = install_sig_alt_stack();

    // The original code was using pthread_cleanup_push/pop, however those are
    // macros in glibc/musl, so we instead use pthread_key_create as it works
    // functionally the same and can call a cleanup function/destructor on both
    // thread exit and cancel
    libc::pthread_setspecific(THREAD_DESTRUCTOR_KEY, alt_stack_mem);
    let thread_rv = user_main(user_arg);

    thread_rv
}

/// Install the alternate signal stack
///
/// Returns a pointer to the memory area we mapped to store the stack only if it
/// was installed successfully, otherwise returns `null`.
unsafe fn install_sig_alt_stack() -> *mut libc::c_void {
    let alt_stack_mem = libc::mmap(
        ptr::null_mut(),
        SIG_STACK_SIZE,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
        -1,
        0,
    );

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
    let rv = libc::sigaltstack(&alt_stack, ptr::null_mut());

    // Attempt to cleanup the mapping if we failed to install the alternate stack
    if rv != 0 {
        assert_eq!(libc::munmap(alt_stack_mem, SIG_STACK_SIZE), 0, "failed to install an alternate signal stack, and failed to unmap the alternate stack memory");
        ptr::null_mut()
    } else {
        alt_stack_mem
    }
}

/// Uninstall the alternate signal stack and unmaps the memory.
#[no_mangle]
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
        libc::sigaltstack(&disable_stack, ptr::null_mut()),
        0,
        "failed to uninstall alternate signal stack"
    );
    assert_eq!(
        libc::munmap(alt_stack_mem, SIG_STACK_SIZE),
        0,
        "failed to unmap alternate stack memory"
    );
}
