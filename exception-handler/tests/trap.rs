mod shared;

#[test]
fn handles_trap() {
    shared::handles_signal(shared::Signal::Trap, sadness_generator::raise_trap);
}
