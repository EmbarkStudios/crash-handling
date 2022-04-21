use mach2::{exception_types as et, mach_types as mt};

/// Information on the exception that caused the crash
#[derive(Copy, Clone)]
pub struct ExceptionInfo {
    /// The exception kind
    pub kind: et::exception_type_t,
    /// The exception code
    pub code: et::mach_exception_data_type_t,
    /// Optional subcode, typically only present for `EXC_BAD_ACCESS` exceptions
    pub subcode: Option<et::mach_exception_data_type_t>,
}

/// Full MacOS crash context
pub struct CrashContext {
    /// The process which crashed
    pub task: mt::task_t,
    /// The thread in the process that crashed
    pub thread: mt::thread_t,
    /// The thread that handled the exception. This may be useful to ignore.
    pub handler_thread: mt::thread_t,
    /// Optional exception information
    pub exception: Option<ExceptionInfo>,
}
