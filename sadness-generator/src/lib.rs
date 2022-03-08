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

    println!("ok...");
    println!("we are crashing by accessing a null reference: {s}");
}

/// Raises `SIGFPE` on unix and a `EXCEPTION_INT_DIVIDE_BY_ZERO` exception on windows
pub fn raise_floating_point_exception() {
    let ohno = unsafe {
        #[cfg(target_arch = "x86_64")]
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
    };

    println!("we are crashing by accessing a null reference: {ohno}");
}

/// Raises `SIGILL` on unix and a `EXCEPTION_ILLEGAL_INSTRUCTION` exception on windows
pub fn raise_illegal_instruction() {
    unsafe {
        #[cfg(target_arch = "x86_64")]
        asm!("ud2");
    }
}

/// Raises `SIGBUS` on unix and who knows what on windows
pub fn raise_bus(path: &str) {
    let path = std::ffi::CString::new(path).unwrap();

    #[cfg(target_os = "linux")]
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
        #[cfg(target_arch = "x86_64")]
        asm!("int3");
    }
}

/// Raises `SIGSEGV` on unix and a `EXCEPTION_STACK_OVERFLOW` exception on windows
pub fn raise_stack_overflow() {
    let mut big_boi = [0u8; 9 * 1024 * 1024];
    big_boi[big_boi.len() - 1] = 1;

    println!("{:?}", &big_boi[big_boi.len() - 20..]);
}
