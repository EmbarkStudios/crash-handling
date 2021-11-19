use minidumper_test::*;

#[test]
fn abort_simple() {
    let md = generate_minidump("abort-simple", Signal::Abort, false);

    assert_minidump(&md, Signal::Abort);
}

#[test]
fn abort_threaded() {
    let md = generate_minidump("abort-threaded", Signal::Abort, true);

    assert_minidump(&md, Signal::Abort);
}
