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
            fn create_minidump_file(
                &self,
            ) -> Result<(std::fs::File, std::path::PathBuf), std::io::Error> {
                let uuid = uuid::Uuid::new_v4();

                let pb = std::path::PathBuf::from(format!("dumps/{}.dmp", uuid));
                Ok((std::fs::File::create(&pb)?, pb))
            }

            /// Called when a crash has been fully written as a minidump to the provided
            /// file. Also returns the full heap buffer as well.
            fn on_minidump_created(
                &self,
                result: Result<minidumper::MinidumpBinary, minidumper::Error>,
            ) {
                match result {
                    Ok(mut md_bin) => {
                        use std::io::Write;
                        let _ = md_bin.file.flush();
                        log::info!("wrote minidump to disk");
                    }
                    Err(e) => {
                        log::error!("failed to write minidump: {:#}", e);
                    }
                }
            }

            fn on_message(&self, kind: u32, buffer: Vec<u8>) {
                log::info!(
                    "kind: {}, message: {}",
                    kind,
                    String::from_utf8(buffer).unwrap()
                );
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

        let exe = std::env::current_exe().expect("unable to find ourselves");

        _server_proc = Some(
            std::process::Command::new(exe)
                .arg("--server")
                .spawn()
                .expect("unable to spawn server process"),
        );

        // Give it time to start
        std::thread::sleep(std::time::Duration::from_millis(100));
    };

    // Register our exception handler
    cfg_if::cfg_if! {
        if #[cfg(any(target_os = "linux", target_os = "android"))] {
            client.send_message(std::num::NonZeroU32::new(1).unwrap(), "mistakes will be made").unwrap();

            let _handler = exception_handler::linux::ExceptionHandler::attach(Box::new(move |crash_context: &exception_handler::CrashContext| {
                log::error!("OH NO");

                // Before we request the crash, send a message to the server
                client.send_message(std::num::NonZeroU32::new(2).unwrap(), "mistakes were made").unwrap();

                dbg!(client.request_dump(crash_context).is_ok())
            })).expect("failed to attach signal handler");

            handler.simulate_signal(exception_handler::Signal::Segv);
        } else {
            unimplemented!("target is not currently implemented");
        }
    }
}
