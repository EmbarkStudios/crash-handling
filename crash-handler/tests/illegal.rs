mod shared;

#[test]
fn handles_illegal_instruction() {
    shared::handles_exception(
        shared::ExceptionKind::Illegal,
        sadness_generator::raise_illegal_instruction,
    );
}
