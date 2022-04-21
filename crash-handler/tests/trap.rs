mod shared;

#[test]
fn handles_trap() {
    shared::handles_exception(shared::ExceptionKind::Trap, sadness_generator::raise_trap);
}
