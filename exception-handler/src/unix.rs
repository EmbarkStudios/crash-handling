#[cfg(feature = "debug-print")]
#[macro_export]
macro_rules! debug_print {
    ($s:literal) => {
        let cstr = concat!($s, "\n");
        $crate::write_stderr(cstr);
    };
}

#[cfg(not(feature = "debug-print"))]
macro_rules! debug_print {
    ($s:literal) => {};
}

/// Writes the specified string directly to stderr. This is safe to be called
/// from within a compromised context.
pub fn write_stderr(s: &'static str) {
    unsafe {
        libc::write(2, s.as_ptr().cast(), s.len());
    }
}
