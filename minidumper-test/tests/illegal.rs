use minidumper_test::*;

#[test]
fn illegal_simple() {
    run_test(Signal::Illegal, 0, false);
}

#[test]
fn illegal_threaded() {
    run_threaded_test(Signal::Illegal);
}
