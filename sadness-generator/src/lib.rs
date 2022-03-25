use std::arch::asm;

pub enum SadnessFlavors {
    /// Raises `SIGABRT` on unix and who knows what on windows
    Abort,
    /// Raises `SIGSEGV` on unix and a `EXCEPTION_ACCESS_VIOLATION` exception on windows
    Segfault,
    /// Raises `SIGFPE` on unix and a `EXCEPTION_INT_DIVIDE_BY_ZERO` exception on windows
    DivideByZero,
    /// Raises `SIGILL` on unix and a `EXCEPTION_ILLEGAL_INSTRUCTION` exception on windows
    Illegal,
    /// Raises `SIGBUS` on unix
    #[cfg(unix)]
    Bus { path: String },
    /// Raises `SIGTRAP` on unix and a `EXCEPTION_BREAKPOINT` exception on windows
    Trap,
    /// Raises `SIGSEGV` on unix and a `EXCEPTION_STACK_OVERFLOW` exception on windows
    StackOverflow {
        /// Raises the signal/exception from a non-[`std::thread::Thread`]
        native_thread: bool,
        /// If using a native thread and there is a signal handler that longjumps,
        /// we can't wait on the thread as we would normally as it would deadlock
        long_jumps: bool,
    },
    /// Raises a [purecall](https://docs.microsoft.com/en-us/cpp/c-runtime-library/reference/purecall?view=msvc-170)
    /// exception
    #[cfg(windows)]
    Purecall,
    /// Raises a [invalid parameter](https://docs.microsoft.com/en-us/cpp/c-runtime-library/reference/invalid-parameter-functions?view=msvc-170)
    /// exception
    #[cfg(windows)]
    InvalidParameter,
}

impl SadnessFlavors {
    /// This only ends one way. Sadness.
    pub fn make_sad(self) {
        match self {
            Self::Abort => raise_abort(),
            Self::Segfault => raise_segfault(),
            Self::DivideByZero => raise_floating_point_exception(),
            Self::Illegal => raise_illegal_instruction(),
            #[cfg(unix)]
            Self::Bus { path } => raise_bus(&path),
            Self::Trap => raise_trap(),
            Self::StackOverflow {
                native_thread,
                long_jumps,
            } => {
                if !native_thread {
                    raise_stack_overflow()
                } else {
                    raise_stack_overflow_in_non_rust_thread(long_jumps)
                }
            }
            #[cfg(windows)]
            Self::Purecall => raise_purecall(),
            #[cfg(windows)]
            Self::InvalidParameter => raise_invalid_parameter(),
        }
    }
}

/// [`SadnessFlavors::Abort`]
pub fn raise_abort() {
    std::process::abort();
}

/// [`SadnessFlavors::Segfault`]
pub fn raise_segfault() {
    let s: &u32 = unsafe {
        // avoid deref_nullptr lint
        fn definitely_not_null() -> *const u32 {
            std::ptr::null()
        }
        &*definitely_not_null()
    };

    println!("we are crashing by accessing a null reference: {s}");
}

/// [`SadnessFlavors::DivideByZero`]
pub fn raise_floating_point_exception() {
    let ohno = unsafe {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            let mut divisor: u32;
            asm!(
                "mov eax, 1",
                "cdq",
                "mov {div:e}, 0",
                "idiv {div:e}",
                div = out(reg) divisor
            );
            divisor
        }
        #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
        {
            // Unfortunately ARM by default will not raise SIGFPE on divide
            // by 0 and just return 0, so we just explicitly raise here for now
            libc::raise(libc::SIGFPE);
            0
        }
    };

    println!("we won't get here because we've raised a floating point exception: {ohno}");
}

/// [`SadnessFlavors::Illegal`]
pub fn raise_illegal_instruction() {
    unsafe {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        asm!("ud2");
        #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
        asm!("udf #0");
    }
}

/// [`SadnessFlavors::Bus`]
#[cfg(unix)]
pub fn raise_bus(path: &str) {
    let path = std::ffi::CString::new(path).unwrap();

    #[cfg(any(target_os = "linux", target_os = "android"))]
    unsafe {
        let bus_fd = libc::open(path.as_ptr(), libc::O_RDWR | libc::O_CREAT, 0o666);
        let mapping = std::slice::from_raw_parts_mut(
            libc::mmap(
                std::ptr::null_mut(),
                128,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                bus_fd,
                0,
            )
            .cast::<u8>(),
            128,
        );

        println!("{}", mapping[1]);
    }
}

/// [`SadnessFlavors::Trap`]
pub fn raise_trap() {
    unsafe {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        asm!("int3");
        #[cfg(target_arch = "arm")]
        asm!(".inst 0xe7f001f0");
        #[cfg(target_arch = "aarch64")]
        asm!(".inst 0xd4200000");
    }
}

/// [`SadnessFlavors::StackOverflow`]
pub fn raise_stack_overflow() {
    let mut big_boi = [0u8; 999 * 1024 * 1024];
    big_boi[big_boi.len() - 1] = 1;

    println!("{:?}", &big_boi[big_boi.len() - 20..]);
}

/// [`SadnessFlavors::StackOverflow`]
///
/// This is raised inside of a non-Rust `std::thread::Thread` to ensure that
/// alternate stacks apply to all threads, even ones not created from Rust
///
pub fn raise_stack_overflow_in_non_rust_thread(uses_longjmp: bool) {
    #[cfg(unix)]
    unsafe {
        let mut native: libc::pthread_t = std::mem::zeroed();
        let mut attr: libc::pthread_attr_t = std::mem::zeroed();

        assert_eq!(
            libc::pthread_attr_setstacksize(&mut attr, 2 * 1024 * 1024),
            0,
            "failed to set thread stack size",
        );

        extern "C" fn thread_start(_arg: *mut libc::c_void) -> *mut libc::c_void {
            raise_stack_overflow();
            std::ptr::null_mut()
        }

        let ret = libc::pthread_create(&mut native, &attr, thread_start, std::ptr::null_mut());

        // We might not get here, but that's ok
        assert_eq!(
            libc::pthread_attr_destroy(&mut attr),
            0,
            "failed to destroy thread attributes"
        );
        assert_eq!(ret, 0, "pthread_create failed");

        // Note if we're doing longjmp shenanigans, we can't do thread join, that
        // has to be handled by the calling code
        if !uses_longjmp {
            assert_eq!(
                libc::pthread_join(native, std::ptr::null_mut()),
                0,
                "failed to join"
            );
        }
    }
}

/// [`SadnessFlavors::StackOverflow`]
#[inline]
pub fn raise_stack_overflow_in_non_rust_thread_normal() {
    raise_stack_overflow_in_non_rust_thread(false);
}

/// [`SadnessFlavors::StackOverflow`]
#[inline]
pub fn raise_stack_overflow_in_non_rust_thread_longjmp() {
    raise_stack_overflow_in_non_rust_thread(true);
}

/// [`SadnessFlavors::Purecall`]
#[cfg(target_os = "windows")]
pub fn raise_purecall() {
    unsafe {
        asm!("call _purecall");
    }
}

/// [`SadnessFlavors::InvalidParameter`]
#[cfg(target_os = "windows")]
pub fn raise_invalid_parameter() {
    extern "C" {
        fn _mbscmp(s1: *const u8, s2: *const u8) -> i32;
    }

    unsafe { _mbscmp(std::ptr::null(), std::ptr::null()) };
}
