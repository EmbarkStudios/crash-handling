mod shared;

#[test]
fn handles_segv() {
    shared::handles_signal(shared::Signal::Segv, sadness_generator::raise_segfault);
}
