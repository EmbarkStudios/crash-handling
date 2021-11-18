use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
struct Command {
    /// Identifier for the client or server, this is required since for tests
    /// we spawn multiple simultaneous processes and want each one to get its
    /// own socket
    #[clap(long)]
    id: String,
    /// The directory where the minidump will be written
    #[clap(short)]
    dump_dir: PathBuf,
    #[clap(subcommand)]
    sub: Subcommand,
}

#[derive(clap::ArgEnum, Clone, Copy)]
enum Signal {
    IllegalInstruction,
    Trap,
    Abort,
    Bus,
    Fpe,
    Segv,
}

#[derive(Parser)]
enum Subcommand {
    /// Runs the client, which will spawn the server automatically and connect
    /// to it before raising the specified signal
    Client {
        /// The signal/exception to raise
        #[clap(possible_values = &Signal::variants())]
        signal: Signal,
    },
    /// Runs the server which is responsible for actually creating a minidump
    /// of the "faulty" client process
    Server,
}

fn main() {
    let cmd = Command::parse();

    match cmd.sub {
        Subcommand::Client { signal } => {
            let mut server_cmd = std::process::Command::new(
                std::env::current_exe().expect("unable to retrieve current executable"),
            );
            server_cmd.args(&[
                "--id",
                &cmd.id,
                "-d",
                &cmd.dump_dir.to_str().expect("non utf-8 dump directory"),
                "server",
            ]);

            server_cmd.spawn();
        }
        Subcommand::Server => {}
    }
}
