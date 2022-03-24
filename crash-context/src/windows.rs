/// This is the same layout as <https://docs.rs/minidump-common/0.10.0/minidump_common/format/struct.MINIDUMP_ASSERTION_INFO.html>
/// but we don't want to require a dependency on minidump-common just for this
#[repr(C)]
pub struct RawAssertionInfo {
    /// The assertion that failed
    pub expression: [u16; 128],
    /// The function containing the failed assertion
    pub function: [u16; 128],
    /// The file file containing the function
    pub file: [u16; 128],
    /// The line number in the file where the assertion is
    pub line: u32,
    /// The type of assertion
    pub kind: u32,
}

/// Contextual crash information on windows.
pub struct CrashContext {
    /// The information on the exception.
    ///
    /// Note that this is a pointer into the actual memory of the crashed process
    pub exception_pointers:
        *const windows_sys::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS,
    /// Assertion info for pure call or invalid parameter errors
    pub assertion_info: Option<*const RawAssertionInfo>,
    /// The top level exception code from the exception_pointers. This is provided
    /// so that external processes don't need to use `ReadProcessMemory` to inspect
    /// the exception code
    pub exception_code: i32,
    /// The thread id on which the exception occurred
    pub thread_id: u32,
}
