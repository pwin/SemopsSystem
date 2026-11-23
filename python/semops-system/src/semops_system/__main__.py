"""The `semops` command, as the wheel installs it.

    semops check ontology.ttl --format sarif --out semops.sarif
    python -m semops_system check ontology.ttl

The same command as the `semops` binary, with every subcommand: `serve`, `query`, `load`,
`validate`, `map`, `check`, `diff`, `repair` and the rest. One difference: under
`semops serve`, `--max-query-memory` is not enforced, because an extension module cannot
install the counting allocator the binary has. For a server, prefer the binary or the
container image.
"""

from __future__ import annotations

import signal
import sys

from . import _native


def main() -> None:
    # The command runs in native code, where Python's own SIGINT handler would only set a
    # flag nothing reads; the default handler lets Ctrl-C end it, as it ends the binary.
    signal.signal(signal.SIGINT, signal.SIG_DFL)
    sys.stdout.flush()
    sys.stderr.flush()
    raise SystemExit(_native.main(["semops", *sys.argv[1:]]))


if __name__ == "__main__":
    main()
