use minidumper_test::*;

#[test]
fn segfault_simple() {
    run_test(Signal::Segv, 0, false);
}

#[test]
fn segfault_threaded() {
    run_threaded_test(Signal::Segv, 32);
}
