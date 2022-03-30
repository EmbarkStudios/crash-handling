/// Contextual crash information on windows.
pub struct CrashContext {
    /// The information on the exception.
    ///
    /// Note that this is a pointer into the actual memory of the crashed process
    pub exception_pointers:
        *const windows_sys::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS,
    /// The top level exception code from the exception_pointers. This is provided
    /// so that external processes don't need to use `ReadProcessMemory` to inspect
    /// the exception code
    pub exception_code: i32,
    /// The thread id on which the exception occurred
    pub thread_id: u32,
}
