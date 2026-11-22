//! `holos-server`: the binary. The server is the `holos_server` library, which
//! `semops serve` runs too. This file holds what only a binary may: the allocator.

mod alloc;

/// The counting allocator, so a query's appetite can be seen before it kills the process.
///
/// Every library crate carries `#![forbid(unsafe_code)]`, and a `GlobalAlloc` cannot be
/// written under it. Keeping the implementation in `alloc.rs`, compiled only into binaries
/// (this one and `semops`), confines it to one file a reviewer can read at once.
#[global_allocator]
static ALLOCATOR: alloc::Counting<std::alloc::System> = alloc::Counting(std::alloc::System);

fn main() -> anyhow::Result<()> {
    // The allocator above is installed by the attribute; this tells the engine the counter
    // is live, so a memory ceiling is enforced rather than silently inert.
    holos_engine::memory::mark_tracking();
    holos_server::main_with(std::env::args_os())
}
