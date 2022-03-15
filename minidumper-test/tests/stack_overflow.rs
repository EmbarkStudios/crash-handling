use minidumper_test::*;

#[test]
fn stack_overflow_simple() {
    run_test(Signal::StackOverflow, 0, false);
}

#[test]
fn stack_overflow_threaded() {
    run_threaded_test(Signal::StackOverflow, 32);
}
