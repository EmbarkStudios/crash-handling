mod shared;

#[test]
fn handles_stack_overflow() {
    #[cfg(unix)]
    shared::handles_signal(
        shared::Signal::Segv,
        sadness_generator::raise_stack_overflow,
    );
    #[cfg(windows)]
    shared::handles_exception(
        shared::ExceptionCode::StackOverflow,
        sadness_generator::raise_stack_overflow,
    );
}

// Not implemented for windows since the point of testing this is to ensure
// an alternate stack is always installed when anything Rust or otherwise does
// a pthread_create
#[cfg(unix)]
#[test]
fn handles_stack_overflow_in_c_thread() {
    shared::handles_signal(
        shared::Signal::Segv,
        sadness_generator::raise_stack_overflow_in_non_rust_thread_longjmp,
    );
}
