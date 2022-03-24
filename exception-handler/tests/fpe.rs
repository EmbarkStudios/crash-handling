mod shared;

#[test]
fn handles_fpe() {
    #[cfg(unix)]
    shared::handles_signal(
        shared::Signal::Fpe,
        sadness_generator::raise_floating_point_exception,
    );
    #[cfg(windows)]
    shared::handles_exception(
        shared::ExceptionCode::Fpe,
        sadness_generator::raise_floating_point_exception,
    );
}
