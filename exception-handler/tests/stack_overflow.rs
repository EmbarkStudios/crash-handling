mod shared;

#[test]
fn handles_stack_overflow() {
    shared::handles_signal(
        shared::Signal::Segv,
        sadness_generator::raise_stack_overflow,
    );
}
