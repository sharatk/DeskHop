//! `deskhop-agent.exe`: input and clipboard on one desktop.
//!
//! `--session` runs on the user desktop; `--winlogon` runs on the secure
//! desktop for UAC prompts and the lock screen. Spawned by the service only.
//! Never touches the network.

#![forbid(unsafe_code)]

use std::process::ExitCode;

enum Mode {
    Session,
    Winlogon,
}

fn parse_mode(mut args: impl Iterator<Item = String>) -> Option<Mode> {
    let mode = match args.next()?.as_str() {
        "--session" => Mode::Session,
        "--winlogon" => Mode::Winlogon,
        _ => return None,
    };
    args.next().is_none().then_some(mode)
}

fn main() -> ExitCode {
    match parse_mode(std::env::args().skip(1)) {
        Some(Mode::Session | Mode::Winlogon) => ExitCode::SUCCESS,
        None => {
            eprintln!("usage: deskhop-agent.exe --session | --winlogon");
            ExitCode::from(2)
        }
    }
}
