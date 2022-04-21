//! Abort is not catchable by a SEH, at least when done via the __fastfail
//! instrinsic which is used by eg. `std::process::abort()`
//!
//! > Critical failures that may have corrupted program state and stack beyond
//! > recovery cannot be handled by the regular exception handling facility.
//! > Use __fastfail to terminate the process using minimal overhead.
//!
//! See <https://docs.microsoft.com/en-us/cpp/intrinsics/fastfail?view=msvc-170>
//! for more information
#![cfg(unix)]

use minidumper_test::*;

#[test]
fn abort_simple() {
    run_test(Signal::Abort, 0, false);
}

#[test]
fn abort_threaded() {
    run_threaded_test(Signal::Abort);
}
