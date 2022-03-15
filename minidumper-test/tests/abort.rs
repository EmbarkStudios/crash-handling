use minidumper_test::*;

#[test]
fn abort_simple() {
    run_test(Signal::Abort, 0, false);
}

#[test]
#[ignore = "slow on CI"]
fn abort_threaded() {
    run_threaded_test(Signal::Abort, 32);
}
