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

#[test]
fn handles_stack_overflow_in_c_thread() {
    #[cfg(unix)]
    shared::handles_signal(
        shared::Signal::Segv,
        sadness_generator::raise_stack_overflow_in_non_rust_thread_longjmp,
    );
}
