//! `tarka`: the binary. The command is the `tarka_cli` library, which `semops map` runs
//! too.

fn main() -> std::process::ExitCode {
    tarka_cli::main_with(std::env::args_os())
}
