use minidumper_test::*;

#[test]
fn abort() {
    capture_output();

    let server = spinup_server("abort-simple");
    run_client("abort-simple", Signal::Abort, false);

    let dump_path = server
        .dump_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("failed to receive dump path");

    assert!(
        std::fs::metadata(&dump_path)
            .expect("failed to read minidump")
            .len()
            > 1024
    );
}

#[test]
fn abort_threaded() {}
