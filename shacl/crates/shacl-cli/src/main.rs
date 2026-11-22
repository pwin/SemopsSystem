//! `shacl`: the binary. The command is the `shacl_cli` library, which `semops validate`
//! runs too.

// Parsing allocates a string per term (millions on a large graph) and the system
// allocator becomes the contention point once that runs across threads. Worth about 25% on
// load, on both the parallel and sequential paths.
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> std::process::ExitCode {
    shacl_cli::main_with(std::env::args_os())
}
