use clap::Parser;
use gitbound::{app, cli::Cli, process::SystemRunner};
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match app::run(cli, &SystemRunner) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}
