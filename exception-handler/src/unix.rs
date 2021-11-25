#[macro_export]
macro_rules! cstr {
    ($s:literal) => {{
        concat!($s, "\n")
    }};
}

#[macro_export]
macro_rules! debug_print {
    ($s:literal) => {
        #[cfg(feature = "debug-print")]
        {
            let cstr = cstr!($s);
            #[allow(unused_unsafe)]
            unsafe {
                libc::write(2, cstr.as_ptr().cast(), cstr.len());
            }
        }
        #[cfg(not(feature = "debug-print"))]
        {}
    };
}
