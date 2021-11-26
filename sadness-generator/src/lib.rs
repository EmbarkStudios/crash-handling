#[link(name = "sadness")]
extern "C" {
    fn sig_fpe();
    fn sig_segv();
    fn sig_ill();
    fn sig_bus(path_ptr: *const u8, path_len: usize);
    fn sig_trap();
}

/// Raises `SIGSABRT` on unix and who knows what on windows
pub fn raise_abort() {
    std::process::abort();
}

/// Raises `SIGSEGV` on unix and a `EXCEPTION_ACCESS_VIOLATION` exception on windows
pub fn raise_segfault() {
    unsafe { sig_segv() }
}

/// Raises `SIGFPE` on unix and a `EXCEPTION_INT_DIVIDE_BY_ZERO` exception on windows
pub fn raise_floating_point_exception() {
    unsafe { sig_fpe() }
}

/// Raises `SIGILL` on unix and a `EXCEPTION_ILLEGAL_INSTRUCTION` exception on windows
pub fn raise_illegal_instruction() {
    unsafe { sig_ill() }
}

/// Raises `SIGBUS` on unix and who knows what on windows
pub fn raise_bus(path: &str) {
    unsafe { sig_bus(path.as_ptr(), path.len()) }
}

/// Raises `SIGTRAP` on unix and a `EXCEPTION_BREAKPOINT` exception on windows
pub fn raise_trap() {
    unsafe { sig_trap() }
}

/// Raises `SIGSEGV` on unix and a `EXCEPTION_STACK_OVERFLOW` exception on windows
pub fn raise_stack_overflow() {
    let mut big_boi = [0u8; 9 * 1024 * 1024];
    big_boi[big_boi.len() - 1] = 1;

    println!("{:?}", &big_boi[big_boi.len() - 20..]);
}
