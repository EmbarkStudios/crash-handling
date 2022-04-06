mod shared;

#[test]
fn handles_fpe() {
    shared::handles_exception(
        shared::ExceptionKind::Fpe,
        sadness_generator::raise_floating_point_exception,
    );
}
