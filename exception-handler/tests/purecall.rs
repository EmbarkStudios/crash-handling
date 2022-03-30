#![cfg(windows)]

mod shared;

#[test]
fn handles_purecall() {
    shared::handles_exception(
        shared::ExceptionCode::Purecall,
        sadness_generator::raise_purecall,
    );
}
