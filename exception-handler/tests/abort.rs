mod shared;

#[test]
fn handles_abort() {
    #[cfg(unix)]
    shared::handles_signal(shared::Signal::Abort, sadness_generator::raise_abort);
    #[cfg(windows)]
    shared::handles_exception(shared::ExceptionCode::Abort, sadness_generator::raise_abort);
}
