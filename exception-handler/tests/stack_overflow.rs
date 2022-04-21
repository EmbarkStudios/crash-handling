mod shared;

#[test]
fn handles_stack_overflow() {
    shared::handles_exception(
        shared::ExceptionKind::StackOverflow,
        sadness_generator::raise_stack_overflow,
    );
}
