#[derive(clap::ArgEnum, Clone, Copy)]
pub enum Signal {
    Illegal,
    Trap,
    Abort,
    Bus,
    Fpe,
    Segv,
    StackOverflow,
}

use std::fmt;
impl fmt::Display for Signal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Illegal => "illegal",
            Self::Trap => "trap",
            Self::Abort => "abort",
            Self::Bus => "bus",
            Self::Fpe => "fpe",
            Self::Segv => "segv",
            Self::StackOverflow => "stack-overflow",
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
            let file = std::fs::File::create(&path)?;

            Ok((file, path))
        }

        fn on_minidump_created(
            &self,
            result: Result<minidumper::MinidumpBinary, minidumper::Error>,
        ) {
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
    use assert_cmd::Command;

    let mut cmd = Command::cargo_bin("client").unwrap();
    cmd.args(&["--id", id, "--signal", &signal.to_string()]);
    if use_thread {
        cmd.arg("--use-thread");
    }

    let assert = cmd.assert().interrupted();
    let output = assert.get_output();

    let stdout = std::str::from_utf8(&output.stdout).expect("invalid stdout");
    let stderr = std::str::from_utf8(&output.stderr).expect("invalid stderr");

    println!("{}", stdout);
    eprintln!("{}", stderr);
}

pub fn capture_output() {
    static SUB: std::sync::Once = std::sync::Once::new();

    SUB.call_once(|| {
        tracing_subscriber::fmt().with_test_writer().init();
    })
}
