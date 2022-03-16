mod shared;

#[test]
fn handles_stack_overflow() {
    shared::handles_signal(
        shared::Signal::Segv,
        sadness_generator::raise_stack_overflow,
    );
}

#[test]
fn handles_stack_overflow_in_c_thread() {
    shared::handles_signal(
        shared::Signal::Segv,
        sadness_generator::raise_stack_overflow_in_non_rust_thread_longjmp,
    );
}
