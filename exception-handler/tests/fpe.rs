mod shared;

#[test]
fn handles_fpe() {
    shared::handles_signal(
        shared::Signal::Fpe,
        sadness_generator::raise_floating_point_exception,
    );
}
