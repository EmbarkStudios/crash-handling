use minidumper_test::*;

#[test]
fn fpe_simple() {
    run_test(Signal::Fpe, 0, false);
}

#[test]
fn fpe_threaded() {
    run_threaded_test(Signal::Fpe, 32);
}
