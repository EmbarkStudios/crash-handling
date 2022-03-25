#![allow(non_camel_case_types)]

use super::HandleDebugExceptions;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Weak,
};
use windows_sys::Win32::{
    Foundation::{
        DBG_PRINTEXCEPTION_C, DBG_PRINTEXCEPTION_WIDE_C, EXCEPTION_BREAKPOINT,
        EXCEPTION_SINGLE_STEP, STATUS_INVALID_PARAMETER, STATUS_NONCONTINUABLE_EXCEPTION,
    },
    System::{
        Diagnostics::Debug::{
            RtlCaptureContext, SetUnhandledExceptionFilter, CONTEXT, EXCEPTION_POINTERS,
            EXCEPTION_RECORD, LPTOP_LEVEL_EXCEPTION_FILTER,
        },
        Threading::GetCurrentThreadId,
    },
};

/// MSVCRT has its own error handling function for invalid parameters to crt functions
/// (eg printf) which instead of returning error codes from the function itself,
/// like one would want, call a handler if specified, or, worse, throw up a dialog
/// if in a GUI!
///
/// [Invalid Parameter Handler](https://docs.microsoft.com/en-us/cpp/c-runtime-library/reference/set-invalid-parameter-handler-set-thread-local-invalid-parameter-handler?view=msvc-170)
///
/// It also has a separate error handling function when calling pure virtuals
/// because why not?
///
/// [Purecall Handler](https://docs.microsoft.com/en-us/cpp/c-runtime-library/reference/get-purecall-handler-set-purecall-handler?view=msvc-170)
extern "C" {
    fn _set_invalid_parameter_handler(
        new_handler: Option<_invalid_parameter_handler>,
    ) -> Option<_invalid_parameter_handler>;
    fn _set_purecall_handler(new_handler: Option<_purecall_handler>) -> Option<_purecall_handler>;
    // This is only available in the debug CRT
    // fn _invalid_parameter(
    //     expression: *const u16,
    //     function: *const u16,
    //     file: *const u16,
    //     line: u32,
    //     reserved: usize,
    // );
    fn _invalid_parameter_noinfo();
}

type _invalid_parameter_handler = unsafe extern "C" fn(
    expression: *const u16,
    function: *const u16,
    file: *const u16,
    line: u32,
    _reserved: usize,
);
type _purecall_handler = unsafe extern "C" fn();

/// Tracks which handler to use when handling an exception since none of the
/// error handler functions take user data
const HANDLER_STACK_INDEX: AtomicUsize = AtomicUsize::new(1);
pub(crate) static HANDLER_STACK: parking_lot::Mutex<Vec<Weak<HandlerInner>>> =
    parking_lot::const_mutex(Vec::new());

pub(crate) struct HandlerInner {
    user_handler: Box<dyn crate::CrashEvent>,
    /// Whether debug exceptions are handled or not
    handle_debug_exceptions: bool,
    /// The previously installed filter before this handler installed its own
    previous_filter: LPTOP_LEVEL_EXCEPTION_FILTER,
    /// The previously installed invalid parameter handler
    previous_iph: Option<_invalid_parameter_handler>,
    /// The previously installed purecall handler
    previous_pch: Option<_purecall_handler>,
}

impl HandlerInner {
    pub(crate) fn new(
        handle_debug_exceptions: HandleDebugExceptions,
        user_handler: Box<dyn crate::CrashEvent>,
    ) -> Self {
        // Note that breakpad has flags so the user can choose which error handlers
        // to install, but for now we just install all of them
        unsafe {
            let previous_filter = SetUnhandledExceptionFilter(Some(handle_exception));

            debug_print!("setting...");
            let previous_iph = _set_invalid_parameter_handler(Some(handle_invalid_parameter));
            let previous_pch = _set_purecall_handler(Some(handle_pure_virtual_call));

            Self {
                user_handler,
                handle_debug_exceptions: handle_debug_exceptions.into(),
                previous_filter,
                previous_iph,
                previous_pch,
            }
        }
    }
}

/// `handle_exception` and `handle_invalid_parameter` are stateless functions
/// without any user provided context, so we need to lookup which handler in
/// our stack to use in case there are multiple of them registered, then
/// restores the state once the handler is finished
///
/// Note this keeps the `HANDLER_STACK` lock for the duration of the scope
struct AutoHandler<'scope> {
    lock: parking_lot::MutexGuard<'scope, Vec<Weak<HandlerInner>>>,
    inner: Arc<HandlerInner>,
}

impl<'scope> AutoHandler<'scope> {
    fn new(lock: parking_lot::MutexGuard<'scope, Vec<Weak<HandlerInner>>>) -> Self {
        let reverse_index = HANDLER_STACK_INDEX.fetch_add(1, Ordering::Relaxed);
        let handler = lock[lock.len() - reverse_index]
            .upgrade()
            .expect("impossible, this can't have dropped");

        // In case another exception occurs while this handler is doing its thing,
        // it should be delivered to the previous filter.
        set_previous_handlers(handler.clone());

        Self {
            lock,
            inner: handler,
        }
    }
}

/// Sets the handlers back to our internal ones
pub(crate) fn set_handlers() {
    unsafe {
        SetUnhandledExceptionFilter(Some(handle_exception));
        _set_invalid_parameter_handler(Some(handle_invalid_parameter));
        _set_purecall_handler(Some(handle_pure_virtual_call));
    }
}

/// Sets the handlers to the previous handlers that were registered when the
/// specified handler was attached
pub(crate) fn set_previous_handlers(handler_inner: Arc<HandlerInner>) {
    unsafe {
        SetUnhandledExceptionFilter(handler_inner.previous_filter);
        _set_invalid_parameter_handler(handler_inner.previous_iph);
        _set_purecall_handler(handler_inner.previous_pch);
    }
}

impl<'scope> std::ops::Deref for AutoHandler<'scope> {
    type Target = HandlerInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'scope> Drop for AutoHandler<'scope> {
    fn drop(&mut self) {
        set_handlers();

        HANDLER_STACK_INDEX.fetch_sub(1, Ordering::Relaxed);
    }
}

/// The handler is not entered, and the OS continues searching for an exception handler.
const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
/// Continue execution at the point of the exception.
const EXCEPTION_CONTINUE_EXECUTION: i32 = -1;
/// Enter the exception handler.
const EXCEPTION_EXECUTE_HANDLER: i32 = 1;

/// Called on the exception thread when an unhandled exception occurs.
/// Signals the exception handler thread to handle the exception.
unsafe extern "system" fn handle_exception(except_info: *const EXCEPTION_POINTERS) -> i32 {
    let lock = HANDLER_STACK.lock();
    let current_handler = AutoHandler::new(lock);

    debug_print!("oh no");

    // Ignore EXCEPTION_BREAKPOINT and EXCEPTION_SINGLE_STEP exceptions.  This
    // logic will short-circuit before calling WriteMinidumpOnHandlerThread,
    // allowing something else to handle the breakpoint without incurring the
    // overhead transitioning to and from the handler thread.  This behavior
    // can be overridden by calling ExceptionHandler::set_handle_debug_exceptions.
    let code = (*(*except_info).ExceptionRecord).ExceptionCode;

    let is_debug_exception = code == EXCEPTION_BREAKPOINT
        || code == EXCEPTION_SINGLE_STEP
        || code == DBG_PRINTEXCEPTION_C
        || code == DBG_PRINTEXCEPTION_WIDE_C;

    if (current_handler.handle_debug_exceptions || !is_debug_exception)
        && current_handler.user_handler.on_crash(&crate::CrashContext {
            exception_pointers: except_info,
            assertion_info: None,
            thread_id: GetCurrentThreadId(),
            exception_code: code,
        })
    {
        // The handler fully handled the exception.  Returning
        // EXCEPTION_EXECUTE_HANDLER indicates this to the system, and usually
        // results in the application being terminated.
        //
        // Note: If the application was launched from within the Cygwin
        // environment, returning EXCEPTION_EXECUTE_HANDLER seems to cause the
        // application to be restarted.
        EXCEPTION_EXECUTE_HANDLER
    } else {
        // There was an exception, it was a breakpoint or something else ignored
        // above, or it was passed to the handler, which decided not to handle it.
        // Give the previous handler a chance to do something with the exception.
        // If there is no previous handler, return EXCEPTION_CONTINUE_SEARCH,
        // which will allow a debugger or native "crashed" dialog to handle the
        // exception.
        if let Some(previous) = current_handler.previous_filter {
            previous(except_info)
        } else {
            EXCEPTION_CONTINUE_SEARCH
        }
    }
}

use crash_context::RawAssertionInfo;

/// Used for assertions that would be raised by the MSVC CRT but are directed to
/// an invalid parameter handler instead.
///
/// <https://docs.rs/minidump-common/0.10.0/minidump_common/format/enum.AssertionType.html#variant.InvalidParameter>
const MD_ASSERTION_INFO_TYPE_INVALID_PARAMETER: u32 = 1;
/// Used for assertions that would be raised by the MSVC CRT but are directed to
/// a pure virtual call handler instead.
///
/// <https://docs.rs/minidump-common/0.10.0/minidump_common/format/enum.AssertionType.html#variant.PureVirtualCall>
const MD_ASSERTION_INFO_TYPE_PURE_VIRTUAL_CALL: u32 = 2;

/// Handler for invalid parameters to CRT functions, this is not an exception so
/// the context (shouldn't be) isn't compromised
///
/// As noted [here](https://docs.microsoft.com/en-us/cpp/c-runtime-library/reference/set-invalid-parameter-handler-set-thread-local-invalid-parameter-handler?view=msvc-170#remarks)
/// the parameters to this function are useless when not linked against the debug
/// CRT, and rust std itself is only ever linked aginst the [non-debug CRT](https://github.com/rust-lang/rust/issues/39016)
/// and you can't really link both the regular and debug CRT in the same application
/// as that results in sadness, so this function just ignores the parameters,
/// unlike the original Breakpad code.
#[no_mangle]
unsafe extern "C" fn handle_invalid_parameter(
    expression: *const u16,
    function: *const u16,
    file: *const u16,
    line: u32,
    reserved: usize,
) {
    debug_print!("yes?");

    let lock = HANDLER_STACK.lock();
    let current_handler = AutoHandler::new(lock);

    debug_print!("excellent");

    let mut assertion: crash_context::RawAssertionInfo = std::mem::zeroed();
    assertion.kind = MD_ASSERTION_INFO_TYPE_INVALID_PARAMETER;

    debug_print!("doing stuff");

    // Make up an exception record for the current thread and CPU context
    // to make it possible for the crash processor to classify these
    // as do regular crashes, and to make it humane for developers to
    // analyze them.
    let mut exception_record: EXCEPTION_RECORD = std::mem::zeroed();
    let mut exception_context: CONTEXT = std::mem::zeroed();
    let exception_ptrs = EXCEPTION_POINTERS {
        ExceptionRecord: &mut exception_record,
        ContextRecord: &mut exception_context,
    };

    debug_print!("capturing context...");
    RtlCaptureContext(&mut exception_context);
    debug_print!("captured...");

    exception_record.ExceptionCode = STATUS_INVALID_PARAMETER;

    // We store pointers to the the expression and function strings,
    // and the line as exception parameters to make them easy to
    // access by the developer on the far side.
    exception_record.NumberParameters = 3;
    exception_record.ExceptionInformation[0] = assertion.expression.as_ptr() as usize;
    exception_record.ExceptionInformation[1] = assertion.file.as_ptr() as usize;
    exception_record.ExceptionInformation[2] = assertion.line as usize;

    debug_print!("calling...");

    if current_handler.user_handler.on_crash(&crate::CrashContext {
        exception_pointers: &exception_ptrs,
        assertion_info: Some(&assertion),
        thread_id: GetCurrentThreadId(),
        exception_code: STATUS_INVALID_PARAMETER,
    }) {
        return;
    }

    if let Some(prev_iph) = current_handler.previous_iph {
        prev_iph(expression, function, file, line, reserved);
    } else {
        // If there's no previous handler, pass the exception back in to the
        // invalid parameter handler's core.  That's the routine that called this
        // function, but now, since this function is no longer registered (and in
        // fact, no function at all is registered), this will result in the
        // default code path being taken: _CRT_DEBUGGER_HOOK and _invoke_watson.
        // Use _invalid_parameter where it exists (in _DEBUG builds) as it passes
        // more information through.  In non-debug builds, it is not available,
        // so fall back to using _invalid_parameter_noinfo.  See invarg.c in the
        // CRT source.

        // _invalid_parameter is only available in the debug CRT
        _invalid_parameter_noinfo();
        // if expression.is_null() && function.is_null() && file.is_null() {
        //     _invalid_parameter_noinfo();
        // } else {
        //     _invalid_parameter(expression, function, file, line, reserved);
        // }
    }

    // The handler either took care of the invalid parameter problem itself,
    // or passed it on to another handler.  "Swallow" it by exiting, paralleling
    // the behavior of "swallowing" exceptions.
    std::process::exit(0);
}

/// Handler for pure virtual function calls, this is not an exception so the
/// context (shouldn't be) isn't compromised
#[no_mangle]
unsafe extern "C" fn handle_pure_virtual_call() {
    let lock = HANDLER_STACK.lock();
    let current_handler = AutoHandler::new(lock);

    let mut assertion: crash_context::RawAssertionInfo = std::mem::zeroed();
    assertion.kind = MD_ASSERTION_INFO_TYPE_PURE_VIRTUAL_CALL;

    // Make up an exception record for the current thread and CPU context
    // to make it possible for the crash processor to classify these
    // as do regular crashes, and to make it humane for developers to
    // analyze them.
    let mut exception_record: EXCEPTION_RECORD = std::mem::zeroed();
    let mut exception_context: CONTEXT = std::mem::zeroed();
    let exception_ptrs = EXCEPTION_POINTERS {
        ExceptionRecord: &mut exception_record,
        ContextRecord: &mut exception_context,
    };

    RtlCaptureContext(&mut exception_context);

    exception_record.ExceptionCode = STATUS_NONCONTINUABLE_EXCEPTION;

    // We store pointers to the the expression and function strings,
    // and the line as exception parameters to make them easy to
    // access by the developer on the far side.
    exception_record.NumberParameters = 3;
    exception_record.ExceptionInformation[0] = assertion.expression.as_ptr() as usize;
    exception_record.ExceptionInformation[1] = assertion.file.as_ptr() as usize;
    exception_record.ExceptionInformation[2] = assertion.line as usize;

    if !current_handler.user_handler.on_crash(&crate::CrashContext {
        exception_pointers: &exception_ptrs,
        assertion_info: Some(&assertion),
        thread_id: GetCurrentThreadId(),
        exception_code: STATUS_NONCONTINUABLE_EXCEPTION,
    }) {
        if let Some(pch) = current_handler.previous_pch {
            // The handler didn't fully handle the exception.  Give it to the
            // previous purecall handler.
            pch();
        } else {
            // If there's no previous handler, return and let _purecall handle it.
            // This will just throw up an assertion dialog.
            return;
        }
    }

    // The handler either took care of the invalid parameter problem itself,
    // or passed it on to another handler. "Swallow" it by exiting, paralleling
    // the behavior of "swallowing" exceptions.
    std::process::exit(0);
}
