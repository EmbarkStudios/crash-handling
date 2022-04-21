#![cfg(unix)]

use minidumper_test::*;

#[test]
fn bus_simple() {
    run_test(Signal::Bus, 0, false);
}

#[test]
fn bus_threaded() {
    run_threaded_test(Signal::Bus);
}
