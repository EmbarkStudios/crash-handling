#![cfg(windows)]

mod shared;

#[test]
fn handles_purecall() {
    shared::handles_exception(
        shared::ExceptionKind::Purecall,
        sadness_generator::raise_purecall,
    );
}
