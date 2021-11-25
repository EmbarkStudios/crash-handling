use minidumper_test::*;

#[test]
fn abort_simple() {
    run_test(Signal::Abort, 0, false);
}

#[test]
fn abort_threaded() {
    for i in 0..32 {
        run_test(Signal::Abort, i, true);
    }
}
