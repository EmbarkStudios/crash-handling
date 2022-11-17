//! This crate's sole purpose is for testing the `minidumper` crate. It is not
//! published and never will be.

#[inline]
pub fn run_test(signal: Signal, counter: u32, use_thread: bool) -> Vec<u8> {
    let id = format!(
        "{}-{}-{}",
        signal,
        counter,
        if use_thread { "threaded" } else { "simple" }
    );
    let md = generate_minidump(&id, signal, use_thread, None);
    assert_minidump(&md, signal);
    md
}

pub fn dump_test(signal: Signal, use_thread: bool, dump_path: Option<PathBuf>) {
    let id = format!(
        "{signal}-0-{}",
        if use_thread { "threaded" } else { "simple" }
    );
    let _md = generate_minidump(&id, signal, use_thread, dump_path);
}

#[derive(clap::ValueEnum, Clone, Copy)]
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
    #[cfg(target_os = "macos")]
    Guard,
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
            #[cfg(target_os = "macos")]
            Self::Guard => "guard",
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

pub fn spinup_server(id: &str, dump_path: Option<PathBuf>) -> Server {
    let dump_path = dump_path.unwrap_or_else(|| make_dump_path(id));

    if dump_path.exists() {
        if let Err(e) = std::fs::remove_file(&dump_path) {
            panic!(
                "failed to remove existing dump file {}: {}",
                dump_path.display(),
                e
            );
        }
    }

    let mut server = minidumper::Server::with_name(id).expect("failed to start server");

    struct Inner {
        _id: String,
        dump_tx: Mutex<mpsc::Sender<PathBuf>>,
        dump_path: PathBuf,
    }

    impl minidumper::ServerHandler for Inner {
        fn create_minidump_file(&self) -> Result<(std::fs::File, PathBuf), std::io::Error> {
            if !self.dump_path.parent().unwrap().exists() {
                let _ = std::fs::create_dir_all(self.dump_path.parent().unwrap());
            }

            let file = std::fs::File::create(&self.dump_path)?;

            Ok((file, self.dump_path.clone()))
        }

        fn on_minidump_created(
            &self,
            result: Result<minidumper::MinidumpBinary, minidumper::Error>,
        ) -> minidumper::LoopAction {
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

            minidumper::LoopAction::Continue
        }

        fn on_message(&self, _kind: u32, _buffer: Vec<u8>) {
            unreachable!("we only test crashes");
        }
    }

    let (tx, rx) = mpsc::channel();

    let inner = Inner {
        _id: id.to_owned(),
        dump_tx: Mutex::new(tx),
        dump_path,
    };

    let exit = Arc::new(AtomicBool::new(false));
    let exit_run_loop = exit.clone();

    let run_loop = std::thread::spawn(move || {
        server
            .run(Box::new(inner), &exit, None)
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
    cmd.args(["--id", id, "--signal", &signal.to_string()]);
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
    });
}

pub fn generate_minidump(
    id: &str,
    signal: Signal,
    use_thread: bool,
    dump_path: Option<PathBuf>,
) -> Vec<u8> {
    capture_output();

    let server = spinup_server(id, dump_path);
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
    let crash_address = exc.get_crash_address(native_os, native_cpu);

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

                //assert_eq!(crash_address, sadness_generator::SEGFAULT_ADDRESS as _);
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
            #[cfg(target_os = "macos")]
            Signal::Guard => {
                unreachable!("macos only");
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
                    errors::ExceptionCodeWindowsAccessType::WRITE
                ));

                assert_eq!(crash_address, sadness_generator::SEGFAULT_ADDRESS as _);
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
            #[cfg(target_os = "macos")]
            Signal::Guard => {
                unreachable!("macos only");
            }
        },
        #[allow(clippy::match_same_arms)]
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
                verify!(CrashReason::MacBadAccessKern(
                    errors::ExceptionCodeMacBadAccessKernType::KERN_MEMORY_ERROR,
                ));
            }
            Signal::Fpe => {
                cfg_if::cfg_if! {
                    if #[cfg(target_arch = "aarch64")] {
                        verify!(CrashReason::MacGeneral(
                            errors::ExceptionCodeMac::EXC_ARITHMETIC,
                            0
                        ));
                    } else if #[cfg(target_arch = "x86_64")] {
                        verify!(CrashReason::MacArithmeticX86(
                            errors::ExceptionCodeMacArithmeticX86Type::EXC_I386_DIV,
                        ));
                    } else {
                        panic!("this target architecture is not supported on mac");
                    }
                }
            }
            Signal::Illegal => {
                cfg_if::cfg_if! {
                    if #[cfg(target_arch = "aarch64")] {
                        verify!(CrashReason::MacBadInstructionArm(
                            errors::ExceptionCodeMacBadInstructionArmType::EXC_ARM_UNDEFINED,
                        ));
                    } else if #[cfg(target_arch = "x86_64")] {
                        verify!(CrashReason::MacBadInstructionX86(
                            errors::ExceptionCodeMacBadInstructionX86Type::EXC_I386_INVOP,
                        ));
                    } else {
                        panic!("this target architecture is not supported on mac");
                    }
                }
            }
            Signal::Segv => {
                verify!(CrashReason::MacBadAccessKern(
                    errors::ExceptionCodeMacBadAccessKernType::KERN_INVALID_ADDRESS,
                ));

                assert_eq!(crash_address, sadness_generator::SEGFAULT_ADDRESS as _);
            }
            Signal::StackOverflow | Signal::StackOverflowCThread => {
                verify!(CrashReason::MacBadAccessKern(
                    errors::ExceptionCodeMacBadAccessKernType::KERN_PROTECTION_FAILURE
                ));
            }
            Signal::Trap => {
                cfg_if::cfg_if! {
                    if #[cfg(target_arch = "aarch64")] {
                        verify!(CrashReason::MacBreakpointArm(
                            errors::ExceptionCodeMacBreakpointArmType::EXC_ARM_BREAKPOINT,
                        ));
                    } else if #[cfg(target_arch = "x86_64")] {
                        verify!(CrashReason::MacBreakpointX86(
                            errors::ExceptionCodeMacBreakpointX86Type::EXC_I386_BPT,
                        ));
                    } else {
                        panic!("this target architecture is not supported on mac");
                    }
                }
            }
            #[cfg(target_os = "macos")]
            Signal::Guard => {
                // Unfortunately the exception code which contains the details
                // on the EXC_GUARD exception is bit packed, so we just manually
                // verify this one
                if let CrashReason::MacGuard(kind, code, guard_id) = crash_reason {
                    // We've tried an operation on a guarded file descriptor
                    assert_eq!(kind, errors::ExceptionCodeMacGuardType::GUARD_TYPE_FD);
                    // The guard identifier used when opening the file
                    assert_eq!(guard_id, sadness_generator::GUARD_ID);

                    // +-------------------+----------------+--------------+
                    // |[63:61] guard type | [60:32] flavor | [31:0] target|
                    // +-------------------+----------------+--------------+
                    assert_eq!(
                        errors::ExceptionCodeMacGuardType::GUARD_TYPE_FD as u8,
                        ((code >> 61) & 0x7) as u8
                    );
                    assert_eq!(1 /* GUARD_CLOSE */, ((code >> 32) & 0x1fffffff) as u32);
                    // The target is just the file descriptor itself which is kind of pointless
                } else {
                    panic!("expected MacGuard crash, crash reason: {:?}", crash_reason);
                }
            }
            #[cfg(windows)]
            Signal::Purecall | Signal::InvalidParameter => {
                unreachable!("windows only");
            }
        },
        _ => unreachable!("apparently we are targeting a new OS"),
    }
}

pub fn run_threaded_test(signal: Signal) {
    use rayon::prelude::*;

    // Github actions run on potatoes, so we limit concurrency when running CI
    // TODO: Macos has unpredictable behavior when multiple processes are handling/dumping
    // exceptions, I don't have time to look into this now, so for now the simple
    // workaround is just to reduce concurrency
    let count = if std::env::var("CI").is_ok() {
        1
    } else if cfg!(target_os = "macos") {
        4
    } else {
        16
    };

    (0..count).into_par_iter().for_each(|i| {
        run_test(signal, i, true);
    });
}
