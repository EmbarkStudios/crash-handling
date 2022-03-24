#![cfg(windows)]

mod shared;

#[test]
fn handles_invalid_param() {
    shared::handles_exception(
        shared::ExceptionCode::InvalidParam,
        sadness_generator::raise_invalid_parameter,
    );
}
