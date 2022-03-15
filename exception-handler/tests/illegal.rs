mod shared;

#[test]
fn handles_illegal_instruction() {
    shared::handles_signal(
        shared::Signal::Ill,
        sadness_generator::raise_illegal_instruction,
    );
}
