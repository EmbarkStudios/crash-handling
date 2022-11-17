use minidumper_test::Signal;

use clap::Parser;

#[derive(Parser)]
struct Command {
    /// The unique identifier for the socket connection and the minidump file
    /// that should be produced when this client crashes
    #[clap(long)]
    id: String,
    /// The signal/exception to raise
    #[clap(long)]
    signal: Signal,
    /// Raises the signal on a separate thread rather than the main thread
    #[clap(long)]
    use_thread: bool,
    /// Waits on a debugger to attach
    #[clap(long)]
    wait_on_debugger: bool,
}

#[allow(unsafe_code)]
fn real_main() -> anyhow::Result<()> {
    let cmd = Command::parse();

    println!("pid: {}", std::process::id());

    let start = std::time::Instant::now();
    let connect_timeout = std::time::Duration::from_secs(2);

    if cmd.wait_on_debugger {
        println!("waiting on debugger");
        // notify_rust has some crazy deps, can be turned on manually if needed...
        // notify_rust::Notification::new()
        //     .summary("Waiting on debugger")
        //     .body(&format!("{} - {}", cmd.id, std::process::id()))
        //     .timeout(0)
        //     .show()
        //     .expect("failed to post notification")
        //     .wait_for_action(|_| {
        //         println!("continuing");
        //     });
    }

    let md_client = loop {
        match minidumper::Client::with_name(&cmd.id) {
            Ok(md_client) => break md_client,
            Err(e) => {
                if std::time::Instant::now() - start > connect_timeout {
                    anyhow::bail!("timed out trying to connect to server process: {:#}", e);
                }
            }
        }
    };

    let _handler = crash_handler::CrashHandler::attach(unsafe {
        crash_handler::make_crash_event(move |cc: &crash_handler::CrashContext| {
            let handled = md_client.request_dump(cc).is_ok();
            crash_handler::CrashEventResult::Handled(handled)
        })
    });

    let signal = cmd.signal;

    let raise_signal = move || {
        // SAFETY: we're about to intentionally crash ourselves via shenanigans,
        // none of this is safe
        unsafe {
            match signal {
                Signal::Illegal => {
                    sadness_generator::raise_illegal_instruction();
                }
                Signal::Trap => {
                    sadness_generator::raise_trap();
                }
                Signal::Abort => {
                    sadness_generator::raise_abort();
                }
                #[cfg(unix)]
                Signal::Bus => {
                    sadness_generator::raise_bus();
                }
                Signal::Fpe => {
                    sadness_generator::raise_floating_point_exception();
                }
                Signal::Segv => {
                    sadness_generator::raise_segfault();
                }
                Signal::StackOverflow => {
                    sadness_generator::raise_stack_overflow();
                }
                Signal::StackOverflowCThread => {
                    #[cfg(unix)]
                    {
                        sadness_generator::raise_stack_overflow_in_non_rust_thread_normal();
                    }
                    #[cfg(windows)]
                    {
                        unreachable!();
                    }
                }
                #[cfg(windows)]
                Signal::Purecall => {
                    sadness_generator::raise_purecall();
                }
                #[cfg(windows)]
                Signal::InvalidParameter => {
                    sadness_generator::raise_invalid_parameter();
                }
                #[cfg(target_os = "macos")]
                Signal::Guard => {
                    sadness_generator::raise_guard_exception();
                }
            }
        }
    };

    let mut threads = Vec::new();

    for _ in 0..10 {
        threads.push(std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::MAX);
        }));
    }

    if cmd.use_thread {
        std::thread::spawn(raise_signal)
            .join()
            .expect("failed to join thread");
    } else {
        raise_signal();
    }

    anyhow::bail!("we should have raised a signal and exited");
}

fn main() {
    // We want this program to crash and have a minidump written, it _shouldn't_
    // have errors that prevent that from happening, so emit an error code if we
    // do encounter an error so that we can fail the test
    if let Err(e) = real_main() {
        eprintln!("error: {:#}", e);

        // When exiting due to a crash, the exit code will be 128 + the integer
        // signal number, at least on unixes
        #[allow(clippy::exit)]
        std::process::exit(222);
    }
}
