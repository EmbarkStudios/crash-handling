//! Abort is not catchable by a SEH, at least when done via the `__fastfail`
//! instrinsic which is used by eg. `std::process::abort()`
//!
//! > Critical failures that may have corrupted program state and stack beyond
//! > recovery cannot be handled by the regular exception handling facility.
//! > Use __fastfail to terminate the process using minimal overhead.
//!
//! See <https://docs.microsoft.com/en-us/cpp/intrinsics/fastfail?view=msvc-170>
//! for more information
#![cfg(unix)]

mod shared;

#[test]
fn handles_abort() {
    shared::handles_crash(shared::SadnessFlavor::Abort);
}
