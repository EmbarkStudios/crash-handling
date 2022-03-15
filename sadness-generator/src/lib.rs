use std::arch::asm;

/// Raises `SIGSABRT` on unix and who knows what on windows
pub fn raise_abort() {
    std::process::abort();
}

/// Raises `SIGSEGV` on unix and a `EXCEPTION_ACCESS_VIOLATION` exception on windows
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

/// Raises `SIGFPE` on unix and a `EXCEPTION_INT_DIVIDE_BY_ZERO` exception on windows
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

/// Raises `SIGILL` on unix and a `EXCEPTION_ILLEGAL_INSTRUCTION` exception on windows
pub fn raise_illegal_instruction() {
    unsafe {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        asm!("ud2");
        #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
        asm!("udf #0");
    }
}

/// Raises `SIGBUS` on unix and who knows what on windows
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

/// Raises `SIGTRAP` on unix and a `EXCEPTION_BREAKPOINT` exception on windows
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

/// Raises `SIGSEGV` on unix and a `EXCEPTION_STACK_OVERFLOW` exception on windows
pub fn raise_stack_overflow() {
    let mut big_boi = [0u8; 999 * 1024 * 1024];
    big_boi[big_boi.len() - 1] = 1;

    println!("{:?}", &big_boi[big_boi.len() - 20..]);
}
