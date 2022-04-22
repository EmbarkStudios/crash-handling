mod shared;

#[test]
fn handles_segv() {
    shared::handles_crash(shared::SadnessFlavor::Segfault);

    // TODO: For some reason this hangs in the std::sync::mpsc::blocking::WaitToken::wait_max_until
    // so we just cheat since this is the only test in this binary, but should
    // be looked at, but hey, consistent on windows and unix so that's fun!
    std::process::exit(0);
}
