mod shared;

#[test]
fn handles_abort() {
    shared::handles_signal(shared::Signal::Abort, sadness_generator::raise_abort);
}
