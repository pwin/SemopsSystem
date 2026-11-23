//! `semops`: the binary. The command is the `semops` library's `main_with`; this file holds
//! what only a binary may, the global allocator.

/// holos-server's counting allocator, the one file of `unsafe` in the workspace, compiled
/// into this binary as it is into holos-server's.
#[path = "../../../holos/crates/holos-server/src/alloc.rs"]
mod alloc;

/// Counting, so `semops serve` enforces `--max-query-memory` as holos-server does, around
/// mimalloc, so `semops validate` parses as fast as `shacl` does.
#[global_allocator]
static ALLOCATOR: alloc::Counting<mimalloc::MiMalloc> = alloc::Counting(mimalloc::MiMalloc);

fn main() -> std::process::ExitCode {
    // The allocator above is installed by the attribute; this tells the engine the counter
    // is live, so a memory ceiling is enforced rather than silently inert.
    holos_engine::memory::mark_tracking();
    semops::main_with(std::env::args_os())
}
