use minidumper_test::*;

#[test]
fn trap_simple() {
    run_test(Signal::Trap, 0, false);
}

#[test]
fn trap_threaded() {
    run_threaded_test(Signal::Trap);
}
