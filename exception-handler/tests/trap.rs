mod shared;

#[test]
fn handles_trap() {
    #[cfg(unix)]
    shared::handles_signal(shared::Signal::Trap, sadness_generator::raise_trap);
    #[cfg(windows)]
    shared::handles_exception(shared::ExceptionCode::Trap, sadness_generator::raise_trap);
}
