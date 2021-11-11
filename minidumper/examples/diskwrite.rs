const SOCKET_NAME: &str = "minidumper-disk-example";

use minidumper::{Client, Server};

fn main() {
    if std::env::args().any(|a| a == "--server") {
        let server = Server::with_name(SOCKET_NAME).expect("failed to create server");

        let ab = std::sync::atomic::AtomicBool::new(false);

        struct Handler;

        impl minidumper::ServerHandler for Handler {
            /// Called when a crash has been received and a backing file needs to be
            /// created to store it.
            fn create_minidump_file(&self) -> Result<std::fs::File, std::io::Error> {
                let uuid = uuid::Uuid::new_v4();

                std::fs::File::create(format!("dumps/{}.dmp", uuid))
            }

            /// Called when a crash has been fully written as a minidump to the provided
            /// file. Also returns the full heap buffer as well.
            fn on_minidump_created(
                &self,
                result: Result<(std::fs::File, Vec<u8>), minidumper::Error>,
            ) {
                match result {
                    Ok((mut file, _)) => {
                        use std::io::Write;
                        let _ = file.flush();
                        log::info!("wrote minidump to disk");
                    }
                    Err(e) => {
                        log::error!("failed to write minidump: {:#}", e);
                    }
                }
            }
        }

        server
            .run(Box::new(Handler), &ab)
            .expect("failed to run server");

        return;
    }

    //let mut _server_proc = None;

    // Attempt to connect to the server
    let client = loop {
        if let Ok(client) = Client::with_name(SOCKET_NAME) {
            break client;
        }

        // let exe = std::env::current_exe().expect("unable to find ourselves");

        // _server_proc = Some(
        //     std::process::Command::new(exe)
        //         .arg("--server")
        //         .spawn()
        //         .expect("unable to spawn server process"),
        // );

        // // Give it time to start
        // std::thread::sleep(std::time::Duration::from_millis(100));
    };

    /// Makes a sad
    fn sigsev() {
        let s: &u32 = unsafe {
            // avoid deref_nullptr lint
            #[inline]
            fn get_ptr() -> *const u32 {
                std::ptr::null()
            }
            &*get_ptr()
        };

        println!("backtrace: {:#?}", backtrace::Backtrace::new());
        println!("we are crashing by accessing a null reference: {}", *s);
    }

    // Register our exception handler
    cfg_if::cfg_if! {
        if #[cfg(any(target_os = "linux", target_os = "android"))] {
            let _handler = exception_handler::linux::ExceptionHandler::attach(Box::new(move |crash_context: &exception_handler::linux::CrashContext| {
                println!("OH NO");
                let cc: &minidumper::CrashContext = unsafe {
                    &*(crash_context as *const exception_handler::linux::CrashContext).cast()
                };

                dbg!(client.request_dump(cc).is_ok())
            })).expect("failed to attach signal handler");

            std::thread::spawn(move || {
                sigsev();
            }).join().unwrap();
        } else {
            unimplemented!("target is not currently implemented");
        }
    }
}
