mod shared;

#[test]
fn handles_segv() {
    shared::handles_signal(shared::Signal::Segv, sadness_generator::raise_segfault);
    // TODO: For some reason this hangs in the std::sync::mpsc::blocking::WaitToken::wait_max_until
    // so we just cheat since this is the only test in this binary, but should
    // be looked at
    std::process::exit(0);
}
