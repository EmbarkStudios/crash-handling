use minidumper_test::*;

#[test]
fn bus_simple() {
    run_test(Signal::Bus, 0, false);
}

#[test]
fn bus_threaded() {
    for i in 0..32 {
        run_test(Signal::Bus, i, true);
    }
}
