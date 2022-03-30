use std::sync::{atomic, Arc};

/// Tests that the user can send and receive their own messages over IPC
#[test]
fn ipc_messages() {
    let name = "ipc_messages";

    let server = minidumper::Server::with_name(name).unwrap();

    struct Message {
        kind: u32,
        msg: String,
    }

    struct Server {
        messages: Arc<parking_lot::Mutex<Vec<Message>>>,
    }

    impl minidumper::ServerHandler for Server {
        fn create_minidump_file(
            &self,
        ) -> Result<(std::fs::File, std::path::PathBuf), std::io::Error> {
            panic!("should not be called");
        }

        fn on_minidump_created(
            &self,
            _result: Result<minidumper::MinidumpBinary, minidumper::Error>,
        ) -> bool {
            panic!("should not be called");
        }

        fn on_message(&self, kind: u32, buffer: Vec<u8>) {
            self.messages.lock().push(Message {
                kind,
                msg: String::from_utf8(buffer).unwrap(),
            });
        }
    }

    let messages = Arc::new(parking_lot::Mutex::new(Vec::new()));

    let server_handler = Server {
        messages: messages.clone(),
    };

    let shutdown = Arc::new(atomic::AtomicBool::new(false));
    let is_shutdown = shutdown.clone();
    let server_loop =
        std::thread::spawn(move || server.run(Box::new(server_handler), &is_shutdown));

    let client = minidumper::Client::with_name(name).unwrap();

    for i in 0..1000 {
        assert!(client.send_message(i, format!("msg #{i}")).is_ok(), "{i}");
    }

    shutdown.store(true, atomic::Ordering::Relaxed);
    server_loop.join().unwrap().unwrap();

    let messages = messages.lock();
    for (i, msg) in (0..1000).zip(messages.iter()) {
        assert_eq!(i, msg.kind);
        assert_eq!(format!("msg #{i}"), msg.msg);
    }
}
