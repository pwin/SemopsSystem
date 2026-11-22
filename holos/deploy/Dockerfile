# Build, from the root of the SemOps System repository:
#         docker build -f holos/deploy/Dockerfile -t holos:latest .
# Run:    docker run --rm -p 7878:7878 -v holos-data:/var/lib/holos holos:latest
#
# Two stages: the build needs a C++ toolchain and libclang for RocksDB; the runtime needs
# neither, so it does not carry them.

# ---- build ----------------------------------------------------------------
# The workspace's rust-version. A newer image builds it too; this one proves the floor.
FROM rust:1.90-bookworm AS build

RUN apt-get update && apt-get install -y --no-install-recommends \
        clang libclang-dev cmake \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src

# The whole workspace, less what the root .dockerignore leaves out. Cargo reads every
# member's manifest before it builds anything, so the SHACL_Engine and tarka crates and the
# shared vendor/ patches must be present even though only two HOLOS binaries are built.
#
# This used to copy `crates` and `testsuites` from HOLOS's own root. `testsuites/` is the
# fetched W3C suites, which only the tests read and which a fresh clone does not have, so
# that COPY failed; and `vendor/`, which the [patch] section needs, was never copied at all.
COPY . .

# `release-abort` is the release profile with `panic = "abort"`, the setting these two
# binaries have always shipped with (see the workspace Cargo.toml).
RUN cargo build --locked --profile release-abort -p holos-cli -p holos-server -p semops

# ---- runtime --------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home /var/lib/holos --shell /usr/sbin/nologin holos \
    && mkdir -p /var/lib/holos/store \
    && chown -R holos:holos /var/lib/holos

COPY --from=build /src/target/release-abort/holos        /usr/local/bin/holos
COPY --from=build /src/target/release-abort/holos-server /usr/local/bin/holos-server
COPY --from=build /src/target/release-abort/semops       /usr/local/bin/semops
COPY holos/deploy/smoke.sh /usr/local/bin/holos-smoke
COPY holos/examples /opt/holos/examples

RUN chmod +x /usr/local/bin/holos-smoke

USER holos
WORKDIR /var/lib/holos
EXPOSE 7878
VOLUME ["/var/lib/holos"]

# 0.0.0.0 inside a container is the container's own namespace, not the host's network.
# Publishing the port is what exposes it, which stays the operator's decision.
#
# Note there is no --trust-forwarded-identity here. Turn it on only when something in
# front of this container sets the identity headers and strips the client's. Overriding
# the command is how you do that; see docker-compose.yml.
ENTRYPOINT ["holos-server"]
CMD ["--listen", "0.0.0.0:7878", "--store", "/var/lib/holos/store"]

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:7878/health || exit 1
