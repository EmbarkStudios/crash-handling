use std::path::PathBuf;

use clap::{ArgEnum, Parser};
use minidumper_test::*;

#[derive(Parser)]
struct Command {
    // The unique identifier for the socket connection and the minidump file
    // that should be produced when this client crashes
    //#[clap(long)]
    //id: String,
    /// The signal/exception to raise
    #[clap(action, long, arg_enum)]
    signal: Option<Signal>,
    /// Raises the signal on a separate thread rather than the main thread
    #[clap(action, long)]
    use_thread: bool,
    /// Waits on a debugger to attach
    #[clap(action, long)]
    wait_on_debugger: bool,

    #[clap(action, long)]
    list: bool,

    #[clap(action, long)]
    dump: Option<PathBuf>,
}

fn main() {
    let cli = Command::parse();

    if cli.list {
        for variant in Signal::value_variants() {
            println!("{}", variant);
        }
    } else if let Some(signal) = cli.signal {
        dump_test(signal, cli.use_thread, cli.dump);
    } else {
        println!("must pass --signal (see available choices with --list)");
    }
}
