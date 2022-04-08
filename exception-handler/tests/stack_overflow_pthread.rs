#![cfg(unix)]

mod shared;

// Not implemented for windows since the point of testing this is to ensure
// an alternate stack is always installed when anything Rust or otherwise does
// a pthread_create
#[test]
fn handles_stack_overflow_in_c_thread() {
    shared::handles_exception(
        shared::ExceptionKind::StackOverflow,
        sadness_generator::raise_stack_overflow_in_non_rust_thread_longjmp,
    );
}
