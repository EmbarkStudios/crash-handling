mod shared;

#[test]
fn handles_illegal_instruction() {
    #[cfg(unix)]
    shared::handles_signal(
        shared::Signal::Ill,
        sadness_generator::raise_illegal_instruction,
    );
    #[cfg(windows)]
    shared::handles_exception(
        shared::ExceptionCode::Illegal,
        sadness_generator::raise_illegal_instruction,
    );
}
