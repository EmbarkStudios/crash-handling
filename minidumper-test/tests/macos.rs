#![cfg(target_os = "macos")]

use minidumper_test::*;

#[test]
fn guard_simple() {
    // Note that since the guard exception uses a particular file path and a
    // guard id, we don't run this threaded
    run_test(Signal::Guard, 0, false);
}
