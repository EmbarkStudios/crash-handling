//! This is ignored on mac when targeting arm as the way sadness-generator does
//! this on that arch is to explicitly raise SIGFPE, however we don't actually
//! handle that signal and rely on the exception port instead
#![cfg(not(all(target_os = "macos", any(target_arch = "arm", target_arch = "aarch64"))))]

use minidumper_test::*;

#[test]
fn fpe_simple() {
    run_test(Signal::Fpe, 0, false);
}

#[test]
fn fpe_threaded() {
    run_threaded_test(Signal::Fpe, 32);
}
