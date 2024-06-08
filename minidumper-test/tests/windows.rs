#![cfg(windows)]

use minidumper_test::*;

#[test]
fn purecall_simple() {
    run_test(Signal::Purecall, 0, false);
}

#[test]
fn purecall_threaded() {
    run_threaded_test(Signal::Purecall);
}

#[test]
fn invalid_param_simple() {
    run_test(Signal::InvalidParameter, 0, false);
}

#[test]
fn invalid_param_threaded() {
    run_threaded_test(Signal::InvalidParameter);
}

#[test]
fn heap_corruption() {
    run_threaded_test(Signal::HeapCorruption);
}
