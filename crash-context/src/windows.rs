/// Full crash context on Windows
pub struct CrashContext {
    /// The information on the exception.
    ///
    /// Note that this is a pointer into the actual memory of the crashed process,
    /// and is a pointer to a [EXCEPTION_POINTERS](https://docs.rs/windows-sys/0.35.0/windows_sys/Win32/System/Diagnostics/Debug/struct.EXCEPTION_POINTERS.html)
    pub exception_pointers: *const std::ffi::c_void,
    /// The top level exception code from the exception_pointers. This is provided
    /// so that external processes don't need to use `ReadProcessMemory` to inspect
    /// the exception code
    pub exception_code: i32,
    /// The pid of the process that crashed
    pub process_id: u32,
    /// The thread id on which the exception occurred
    pub thread_id: u32,
}
