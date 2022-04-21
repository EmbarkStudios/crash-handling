//! Not test for windows/macos since the point of testing this is to ensure
//! an alternate stack is always installed when anything Rust or otherwise does
//! a pthread_create. Windows doesn't use pthreads, and macos send stack overflow
//! exceptions to a completely separate thread from the one that overflowed,
//! avoiding the problem altogether
#![cfg(all(unix, not(target_os = "macos")))]

mod shared;

#[test]
fn handles_stack_overflow_in_c_thread() {
    shared::handles_exception(
        shared::ExceptionKind::StackOverflow,
        sadness_generator::raise_stack_overflow_in_non_rust_thread_longjmp,
    );
}
