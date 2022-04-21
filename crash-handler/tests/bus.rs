#![cfg(unix)]

mod shared;

#[test]
fn handles_bus() {
    shared::handles_exception(shared::ExceptionKind::Bus, sadness_generator::raise_bus);
}
