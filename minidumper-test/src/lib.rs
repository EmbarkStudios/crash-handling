#[inline]
pub fn run_test(signal: Signal, counter: u32, use_thread: bool) -> Vec<u8> {
    let id = format!(
        "{}-{}-{}",
        signal,
        counter,
        if use_thread { "threaded" } else { "simple" }
    );
    let md = generate_minidump(&id, signal, use_thread);
    assert_minidump(&md, signal);
    md
}

#[derive(clap::ArgEnum, Clone, Copy)]
pub enum Signal {
    #[cfg(unix)]
    Abort,
    #[cfg(unix)]
    Bus,
    Fpe,
    Illegal,
    Segv,
    StackOverflow,
    StackOverflowCThread,
    Trap,
    #[cfg(windows)]
    Purecall,
    #[cfg(windows)]
    InvalidParameter,
}

use std::fmt;
impl fmt::Display for Signal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            #[cfg(unix)]
            Self::Abort => "abort",
            #[cfg(unix)]
            Self::Bus => "bus",
            Self::Fpe => "fpe",
            Self::Illegal => "illegal",
            Self::Segv => "segv",
            Self::StackOverflow => "stack-overflow",
            Self::StackOverflowCThread => "stack-overflow-c-thread",
            Self::Trap => "trap",
            #[cfg(windows)]
            Self::Purecall => "purecall",
            #[cfg(windows)]
            Self::InvalidParameter => "invalid-parameter",
        })
    }
}

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
};

pub struct Server {
    pub id: String,
    pub dump_rx: mpsc::Receiver<PathBuf>,
    exit_run_loop: Arc<AtomicBool>,
    run_loop: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.exit_run_loop.store(true, Ordering::Relaxed);
        if let Some(jh) = self.run_loop.take() {
            jh.join().expect("failed to join server thread");
        }
    }
}

#[inline]
fn make_dump_path(id: &str) -> PathBuf {
    PathBuf::from(format!(".dumps/{}.dmp", id))
}

pub fn spinup_server(id: &str) -> Server {
    let dump_path = make_dump_path(id);

    if dump_path.exists() {
        if let Err(e) = std::fs::remove_file(&dump_path) {
            panic!(
                "failed to remove existing dump file {}: {}",
                dump_path.display(),
                e
            );
        }
    }

    let server = minidumper::Server::with_name(id).expect("failed to start server");

    struct Inner {
        id: String,
        dump_tx: Mutex<mpsc::Sender<PathBuf>>,
    }

    impl minidumper::ServerHandler for Inner {
        fn create_minidump_file(&self) -> Result<(std::fs::File, PathBuf), std::io::Error> {
            let path = make_dump_path(&self.id);

            if !path.parent().unwrap().exists() {
                let _ = std::fs::create_dir_all(path.parent().unwrap());
            }

            let file = std::fs::File::create(&path)?;

            Ok((file, path))
        }

        fn on_minidump_created(
            &self,
            result: Result<minidumper::MinidumpBinary, minidumper::Error>,
        ) -> bool {
            let md_bin = result.expect("failed to write minidump");
            md_bin
                .file
                .sync_all()
                .expect("failed to flush minidump file");

            self.dump_tx
                .lock()
                .expect("unable to acquire lock")
                .send(md_bin.path)
                .expect("couldn't send minidump path");

            false
        }

        fn on_message(&self, _kind: u32, _buffer: Vec<u8>) {
            unimplemented!();
        }
    }

    let (tx, rx) = mpsc::channel();

    let inner = Inner {
        id: id.to_owned(),
        dump_tx: Mutex::new(tx),
    };

    let exit = Arc::new(AtomicBool::new(false));
    let exit_run_loop = exit.clone();

    let run_loop = std::thread::spawn(move || {
        server
            .run(Box::new(inner), &exit)
            .expect("failed to run server loop");
    });

    Server {
        id: id.to_owned(),
        dump_rx: rx,
        exit_run_loop,
        run_loop: Some(run_loop),
    }
}

pub fn run_client(id: &str, signal: Signal, use_thread: bool) {
    use std::env;

    // Adapted from
    // https://github.com/rust-lang/cargo/blob/485670b3983b52289a2f353d589c57fae2f60f82/tests/testsuite/support/mod.rs#L507
    let mut cmd_path = env::current_exe().expect("failed to get exe path");
    cmd_path.pop();
    if cmd_path.ends_with("deps") {
        cmd_path.pop();
    }

    cmd_path.push("crash-client");
    if cfg!(target_os = "windows") {
        cmd_path.set_extension("exe");
    }

    println!("running client: {}", cmd_path.display());
    let mut cmd = std::process::Command::new(&cmd_path);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cmd.args(&["--id", id, "--signal", &signal.to_string()]);
    if use_thread {
        cmd.arg("--use-thread");
    }

    let wait_for_debugger = env::var("DEBUG").is_ok();
    if wait_for_debugger {
        cmd.arg("--wait-on-debugger");
    }

    let child = cmd.spawn().expect("failed to run crash-client");
    let output = child.wait_with_output().expect("failed to wait for output");

    let stdout = std::str::from_utf8(&output.stdout).expect("invalid stdout");
    let stderr = std::str::from_utf8(&output.stderr).expect("invalid stderr");

    println!("{}", stdout);
    eprintln!("{}", stderr);

    // Ensure it was interrupted and did not exit properly
    #[cfg(unix)]
    assert!(output.status.code().is_none());
    #[cfg(windows)]
    {
        // TODO: check that the status code matches the underlying error value
        println!("client exited with {:?}", output.status.code());
    }
}

#[inline]
pub fn capture_output() {
    static SUB: std::sync::Once = std::sync::Once::new();

    SUB.call_once(|| {
        tracing_subscriber::fmt().with_test_writer().init();
    })
}

pub fn generate_minidump(id: &str, signal: Signal, use_thread: bool) -> Vec<u8> {
    capture_output();

    let server = spinup_server(id);
    run_client(id, signal, use_thread);

    let dump_path = server
        .dump_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("failed to receive dump path");

    match std::fs::read(&dump_path) {
        Ok(buf) => buf,
        Err(e) => {
            panic!(
                "failed to read minidump from {}: {}",
                dump_path.display(),
                e
            );
        }
    }
}

pub use minidump::system_info::{Cpu, Os};

#[inline]
pub fn get_native_os() -> Os {
    cfg_if::cfg_if! {
        if #[cfg(target_os = "linux")] {
            Os::Linux
        } else if #[cfg(target_os = "windows")] {
            Os::Windows
        } else {
            Os::MacOs
        }
    }
}

#[inline]
pub fn get_native_cpu() -> Cpu {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "x86_64")] {
            Cpu::X86_64
        } else if #[cfg(target_arch = "x86")] {
            Cpu::X86
        } else if #[cfg(target_arch = "aarch64")] {
            Cpu::Arm64
        } else if #[cfg(target_arch = "arm")] {
            Cpu::Arm
        }
    }
}

pub fn assert_minidump(md_buf: &[u8], signal: Signal) {
    use minidump::CrashReason;
    use minidump_common::errors;

    let md = minidump::Minidump::read(md_buf).expect("failed to parse minidump");

    let exc: minidump::MinidumpException<'_> =
        md.get_stream().expect("unable to find exception stream");

    let native_os = get_native_os();
    let native_cpu = get_native_cpu();

    let crash_reason = exc.get_crash_reason(native_os, native_cpu);

    macro_rules! verify {
        ($expected:pat) => {
            assert!(
                matches!(crash_reason, $expected),
                "crash reason: {:?}",
                crash_reason
            );
        };
    }

    match native_os {
        Os::Linux => match signal {
            #[cfg(unix)]
            Signal::Abort => {
                verify!(CrashReason::LinuxGeneral(
                    errors::ExceptionCodeLinux::SIGABRT,
                    _
                ));
            }
            #[cfg(unix)]
            Signal::Bus => {
                verify!(CrashReason::LinuxSigbus(
                    errors::ExceptionCodeLinuxSigbusKind::BUS_ADRERR
                ));
            }
            Signal::Fpe => {
                verify!(CrashReason::LinuxSigfpe(
                    errors::ExceptionCodeLinuxSigfpeKind::FPE_INTDIV
                ));
            }
            Signal::Illegal => {
                verify!(CrashReason::LinuxSigill(
                    errors::ExceptionCodeLinuxSigillKind::ILL_ILLOPN
                ));
            }
            Signal::Segv => {
                verify!(CrashReason::LinuxSigsegv(
                    errors::ExceptionCodeLinuxSigsegvKind::SEGV_MAPERR
                ));
            }
            Signal::StackOverflow | Signal::StackOverflowCThread => {
                // Not sure if there is a way to work around this, but on Linux it seems that a stack overflow
                // on the main thread is always reported as a SEGV_MAPERR rather than a SEGV_ACCERR like for
                // non-main threads, so we just accept either ¯\_(ツ)_/¯
                verify!(CrashReason::LinuxSigsegv(
                    errors::ExceptionCodeLinuxSigsegvKind::SEGV_ACCERR
                        | errors::ExceptionCodeLinuxSigsegvKind::SEGV_MAPERR
                ));
            }
            Signal::Trap => {
                verify!(CrashReason::LinuxGeneral(
                    errors::ExceptionCodeLinux::SIGTRAP,
                    _
                ));
            }
            #[cfg(windows)]
            Signal::Purecall | Signal::InvalidParameter => {
                unreachable!("windows only");
            }
        },
        Os::Windows => match signal {
            Signal::Fpe => {
                verify!(CrashReason::WindowsGeneral(
                    errors::ExceptionCodeWindows::EXCEPTION_INT_DIVIDE_BY_ZERO
                ));
            }
            Signal::Illegal => {
                verify!(CrashReason::WindowsGeneral(
                    errors::ExceptionCodeWindows::EXCEPTION_ILLEGAL_INSTRUCTION
                ));
            }
            Signal::Segv => {
                verify!(CrashReason::WindowsAccessViolation(
                    errors::ExceptionCodeWindowsAccessType::READ
                ));
            }
            Signal::StackOverflow | Signal::StackOverflowCThread => {
                verify!(CrashReason::WindowsGeneral(
                    errors::ExceptionCodeWindows::EXCEPTION_STACK_OVERFLOW
                ));
            }
            Signal::Trap => {
                verify!(CrashReason::WindowsGeneral(
                    errors::ExceptionCodeWindows::EXCEPTION_BREAKPOINT
                ));
            }
            #[cfg(windows)]
            Signal::Purecall => {
                assert_eq!(crash_reason, CrashReason::from_windows_code(0xc0000025));
            }
            #[cfg(windows)]
            Signal::InvalidParameter => {
                assert_eq!(crash_reason, CrashReason::from_windows_error(0xc000000d));
            }
            #[cfg(unix)]
            Signal::Bus | Signal::Abort => {
                unreachable!();
            }
        },
        Os::MacOs => match signal {
            #[cfg(unix)]
            Signal::Abort => {
                verify!(CrashReason::MacGeneral(
                    errors::ExceptionCodeMac::EXC_SOFTWARE,
                    0x10003, // EXC_SOFT_SIGNAL
                ));
            }
            #[cfg(unix)]
            Signal::Bus => {
                verify!(CrashReason::MacGeneral(
                    errors::ExceptionCodeMac::EXC_BAD_ACCESS,
                    0
                ));
            }
            Signal::Fpe => {
                verify!(CrashReason::MacGeneral(
                    errors::ExceptionCodeMac::EXC_ARITHMETIC,
                    0
                ));
            }
            Signal::Illegal => {
                verify!(CrashReason::MacGeneral(
                    errors::ExceptionCodeMac::EXC_BAD_INSTRUCTION,
                    0
                ));
            }
            Signal::Segv => {
                verify!(CrashReason::MacGeneral(
                    errors::ExceptionCodeMac::EXC_BAD_ACCESS,
                    0
                ));
            }
            Signal::StackOverflow | Signal::StackOverflowCThread => {
                verify!(CrashReason::MacGeneral(
                    errors::ExceptionCodeMac::EXC_BAD_ACCESS,
                    _ // This will be the an actual address
                ));
            }
            Signal::Trap => {
                verify!(CrashReason::MacGeneral(
                    errors::ExceptionCodeMac::EXC_BREAKPOINT,
                    _ // EXC_BREAKPOINT says "details in the code field" but doesn't elaborate what that means and I'm too lazy to look at more mac source code right now
                ));
            }
            #[cfg(windows)]
            Signal::Purecall | Signal::InvalidParameter => {
                unreachable!("windows only");
            }
        },
        _ => unimplemented!(),
    }
}

pub fn run_threaded_test(signal: Signal, count: u32) {
    use rayon::prelude::*;

    // Github actions run on potatoes, so we limit concurrency when running CI
    let count = if std::env::var("CI").is_ok() {
        1
    } else {
        count
    };

    (0..count).into_par_iter().for_each(|i| {
        run_test(signal, i, true);
    });
}
