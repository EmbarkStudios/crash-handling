#![cfg(windows)]

mod shared;

#[test]
fn handles_invalid_param() {
    shared::handles_exception(
        shared::ExceptionKind::InvalidParam,
        sadness_generator::raise_invalid_parameter,
    );
}
