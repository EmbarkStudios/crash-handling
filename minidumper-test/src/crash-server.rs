use clap::Parser;

#[derive(Parser)]
struct Command {
    /// The unique identifier for the socket connection and the minidump file
    /// that should be produced when this clietn
    #[clap(long)]
    id: String,
}

fn main() {
    let cmd = Command::parse();

    println!("pid: {}", std::process::id());

    let server = minidumper_test::spinup_server(&cmd.id);

    let dump_path = server.dump_rx.recv().expect("failed to receive dump path");

    println!("dump written to {}", dump_path.display());
}
