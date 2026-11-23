# Installing {#installing}

There are seven ways to install SemOps System, and they suit different jobs. Pick from the
table, then read the section for your choice. Each ends with a command that tells you the
install worked.

| Option | What you get | Choose it for |
|---|---|---|
| [From source](#from-source) | All five commands: `holos`, `holos-server`, `semops`, `shacl`, `tarka` | A workstation or build server with Rust, and the newest version |
| [A container image](#container) | `semops`, with `holos-server`, `holos`, `shacl` and `tarka` as links to it | Servers, Kubernetes, Azure |
| [Python](#python) | The store, the validator and the mapper as Python modules | Notebooks, scripts, data platforms |
| [JavaScript and WebAssembly](#javascript) | The store, the validator and the quality checks for Node and the browser | Web applications, editors |
| [Release binaries](#release-binaries) | The `semops` binary, prebuilt, with the engines' command names as links | A machine with no Rust toolchain |
| [As a service](#service) | `holos-server` started by systemd or Windows | A long-running endpoint on one machine |
| [On Azure](#azure-install) | The server on AKS or Container Apps | Cloud deployments |

::: note
The release binaries, the container image and the Python package are all built from one
tagged release of this workspace, 0.30.0 at the time of writing, so they agree with each
other. A build from source has whatever has been merged since. The npm packages and the
Azure templates come from the repository for now; [What is not finished](12-not-finished.md)
says what is still to come.
:::

## From source {#from-source}

A build from source gives every command at the version this manual describes.

### What you need first

| | Linux | macOS | Windows |
|---|---|---|---|
| Rust | 1.90 or later from [rustup.rs](https://rustup.rs); 1.95 or later to build `tarka` | Same | Same |
| A C++ compiler | `build-essential` (Debian, Ubuntu) or `gcc-c++` (Fedora, RHEL) | Xcode command line tools: `xcode-select --install` | Visual Studio Build Tools, with the "Desktop development with C++" workload |
| libclang | `clang libclang-dev` | Included with the Xcode tools | LLVM from [llvm.org](https://llvm.org) or `winget install LLVM.LLVM` |
| CMake | `cmake` | `brew install cmake` | Included with the Build Tools |

The C++ compiler, libclang and CMake are for RocksDB, the store's on-disk backend. Without
them you can still build everything except a persistent store: see
[without RocksDB](#without-rocksdb) below.

On Debian or Ubuntu the whole list is one command:

```sh
sudo apt-get install -y build-essential clang libclang-dev cmake
```

### Build

```sh
git clone https://github.com/pwin/SemopsSystem.git
cd SemopsSystem
cargo build --release
```

The first build takes a while: RocksDB and Polars are large. The commands land in
`target/release/`.

### Install the commands

`cargo install` copies a command into `~/.cargo/bin`, which rustup puts on your `PATH`.
Install the ones you need:

```sh
cargo install --locked --path crates/semops                # semops
cargo install --locked --path holos/crates/holos-cli       # holos
cargo install --locked --path holos/crates/holos-server    # holos-server
cargo install --locked --path shacl/crates/shacl-cli       # shacl
cargo install --locked --path tarka/crates/tarka-cli       # tarka (Rust 1.95)
```

### Without RocksDB {#without-rocksdb}

Where RocksDB's C++ build will not compile, build the server without it. It then keeps its
store in memory, loads files with `--data` at start-up, and refuses `--store`:

```sh
cargo install --locked --path holos/crates/holos-server --no-default-features
```

### Check it worked

```sh
semops --version
holos-server --version
shacl --version
```

```{.output}
semops 0.30.0
holos-server 0.30.0 (<commit>)
shacl 0.30.0
```

## A container image {#container}

The image holds the one `semops` binary, with `holos-server`, `holos`, `shacl` and `tarka`
as links to it. It runs the server as a non-root user (uid 10001), and keeps its store in
the volume `/var/lib/holos`. Build it from the workspace root:

```sh
docker build -f holos/deploy/Dockerfile -t semops:0.30.0 .
```

Run the server, with its store in a named volume:

```sh
docker run -d --name semops -p 7878:7878 -v semops-data:/var/lib/holos semops:0.30.0
curl -s http://127.0.0.1:7878/health
```

```{.output}
ok
```

Run the other commands from the same image by replacing the entry point. This checks an
ontology in the current directory:

```sh
docker run --rm --entrypoint semops -v "$PWD:/work" -w /work semops:0.30.0 check ontology.ttl
```

`holos/deploy/docker-compose.yml` runs the server behind a front door that sets the
identity headers, as [chapter 9](09-security.md) describes.

Each SemOps System release publishes the image as `ghcr.io/pwin/semops`, for amd64 and
arm64, tagged with the version, so there is no need to build it:

```sh
docker pull ghcr.io/pwin/semops:0.30.0
```

For Kubernetes or Azure, pin it by digest, and mirror it into your own registry if your
cluster pulls only from there.

## Python {#python}

```sh
pip install semops-system
```

It needs Python 3.11 or later. One native wheel holds the store, the SHACL engine, tarka
and the quality checks, built from this workspace:

```python
import semops_system as so

store = so.holos.Store()                 # in memory; Store("path") keeps it on disk
store.update('INSERT DATA { <https://example.org/a> <https://example.org/b> "c" }')
print(store.query('ASK { ?s ?p "c" }'))   # True

report = so.shacl.Shapes.from_file("shapes.ttl").validate_file("data.ttl")
print(report.conforms)

run = so.check("ontology.ttl")           # the quality checks, as `semops check` runs them
print(len(run["findings"]), run["complete"])
```

`so.holos`, `so.shacl` and `so.tarka` are the modules the `holosdb`, `shacl` and
`tarka-rdf` wheels carry, and each one's own documentation applies.

The wheel also installs the `semops` command, with every subcommand, so a machine with
Python needs no Rust toolchain to run it. The one difference from the binary: under
`semops serve`, `--max-query-memory` is not enforced, because an extension module cannot
install the counting allocator that needs. Run a server from the binary or the image. `so.check` returns
`findings`, a list of dicts, and `complete`, which is false if a check could not run.

There are wheels for Linux (x86_64 and aarch64), macOS (Apple Silicon and Intel) and
Windows (x64). Elsewhere pip builds from the source distribution, which needs Rust, a C++
compiler and libclang. 0.29.0 was the first native release. The 0.29.0.dev1 release before
it was a bridge to the separately released engines, and code written against it keeps
working.

The engines are also on PyPI separately: `pip install holosdb`, `pip install shacl`,
`pip install tarka-rdf`.

## JavaScript and WebAssembly {#javascript}

`semops-wasm` puts the store, the validator and the quality checks in one WebAssembly
module, in a build for bundlers (`semops-wasm`) and a build for Node (`semops-wasm-node`).
It is built in this workspace and is not on npm yet. Build it, and install the Node build
into a project:

```sh
node crates/semops-wasm/build.mjs
npm install /path/to/SemopsSystem/crates/semops-wasm/pkg-node
```

The quality checks then run in the process, as `semops check` runs them:

```js
const fs = require('node:fs');
const semops = require('semops-wasm-node');
const run = semops.check(fs.readFileSync('shop.ttl', 'utf8'), 'turtle');
for (const f of run.findings) console.log(f.severity, f.checkId, f.focusNode);
console.log(`${run.findings.length} findings, complete: ${run.complete}`);
```

```{.output}
Warning QUA-001 https://example.org/shop#Bookshop
Warning QUA-004 https://example.org/shop
…
Info QUA-007 https://example.org/shop
10 findings, complete: true
```

On the same file, `semops check` finds the same ten. `Store` and the validator's
`Validator`, `Report` and `validateTurtle` keep the names they have in the engines' own
packages, which are on npm now:

```sh
npm install holos-wasm        # the store, for the browser and bundlers
npm install holos-wasm-node   # the store, for Node.js
npm install shacl-wasm        # the validator, for the browser
npm install shacl-wasm-node   # the validator, for Node.js
```

All of them run entirely in the page or the process, with no server. The Turtle Editor
Viewer the Semantechs courses use is built on the engines' packages. Each package's `.d.ts`
file is its reference.

## Release binaries {#release-binaries}

Each SemOps System release carries the `semops` binary, in an archive per platform:
Linux (x86-64 and arm64), macOS (Apple Silicon and Intel) and Windows (x86-64). The Unix
archives include `holos-server`, `holos`, `shacl` and `tarka` as links to it. On Windows,
copy `semops.exe` to any of those names to get that command. The releases are at
[pwin/SemopsSystem](https://github.com/pwin/SemopsSystem/releases), 0.30.0 the first.

Unpack the archive and put the command on your `PATH`. `SHA256SUMS`, beside the archives,
lets you check what you downloaded before you run it:

```sh
sha256sum -c SHA256SUMS --ignore-missing
semops --version
```

The engines' own repositories also publish their commands, released on their own before
the workspace:

| Engine | Commands | Platforms | Releases |
|---|---|---|---|
| HOLOS 0.27.0 | `holos`, `holos-server` | Linux x86-64 and arm64, macOS Intel and Apple Silicon, Windows x86-64 | [pwin/triplestore](https://github.com/pwin/triplestore/releases) |
| SHACL engine 0.3.9 | `shacl` | Linux x86-64, macOS Apple Silicon, Windows x86-64 | [pwin/SHACL_Engine](https://github.com/pwin/SHACL_Engine/releases) |
| tarka 0.3.1 | `tarka` | Linux x86-64, macOS Apple Silicon, Windows x86-64 | [pwin/tarka](https://github.com/pwin/tarka/releases) |

These are the engines as they were released on their own. They do not include `semops`,
placetime, or the security settings of [chapter 9](09-security.md).

## As a service {#service}

`holos/deploy/` holds the scripts that run `holos-server` as a long-running service. One
file, `holos.env`, holds the settings, and the scripts turn it into the server's flags. Put
your own values in `holos.env.local` beside it, which the scripts read afterwards.

### Linux, with systemd

Build from source first, then:

```sh
sudo useradd --system --home /var/lib/holos --shell /usr/sbin/nologin holos
sudo mkdir -p /var/lib/holos/store /opt/holos/target/release
sudo cp target/release/holos target/release/holos-server /opt/holos/target/release/
sudo cp -r holos/deploy /opt/holos/
sudo chown -R holos:holos /var/lib/holos
sudo cp holos/deploy/holos.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now holos
journalctl -u holos -n 20
```

Edit `/opt/holos/deploy/holos.env.local` and run `sudo systemctl restart holos` to change a
setting.

### Windows

From an elevated PowerShell 7, in the workspace:

```powershell
holos\deploy\install-service.ps1             # install, with the settings in holos.env
holos\deploy\install-service.ps1             # again, after changing holos.env.local
holos\deploy\install-service.ps1 -Remove     # uninstall
```

A Windows service has no start script, so the settings are turned into flags when you run
`install-service.ps1`, and a change needs the script run again. The service discards the
server's diagnostics; for logs, run it under NSSM or a scheduled task that redirects them.

## On Azure {#azure-install}

`deploy/azure/` holds a Helm chart for AKS and a Bicep template for Azure Container Apps.
[Running on Azure](10-azure.md) helps you choose between them and between the storage
options, NFS shares among them.

```sh
helm install holos deploy/azure/aks/holos -n semops --create-namespace
```
