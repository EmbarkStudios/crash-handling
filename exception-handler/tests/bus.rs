mod shared;

#[test]
fn handles_bus() {
    // TODO: for some reason raising an _actual_ SIGBUS with an unaligned
    // memory access is _not_ sent to our signal handler at all, so for now
    // just raise it manually and come back to this. This is _somewhat_ ok
    // as SIGBUS is rarer compared to SIGSEGV
    shared::handles_signal(shared::Signal::Bus, || unsafe {
        libc::raise(shared::Signal::Bus as i32);
    });
}
