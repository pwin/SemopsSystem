//! `holos`: the binary. The command is the `holos_cli` library, which `semops` runs too.

fn main() -> std::process::ExitCode {
    holos_cli::main_with(std::env::args_os())
}
