#!/usr/bin/env sh
# Fetches every third-party test suite the workspace's conformance tests read, each at the
# commit the conformance baselines (holos/conformance/*.failures) were recorded against:
#
#   holos/testsuites/rdf-tests    W3C RDF 1.1/1.2 and SPARQL 1.0/1.1/1.2
#   holos/testsuites/data-shapes  W3C SHACL 1.0 and 1.2, for HOLOS's two validators
#
# Pinned, not HEAD. The W3C suites change: between the baselines (August 2026) and November
# 2026 the RDF 1.2 suite was reorganised from 1405 tests to 380 and the SHACL 1.2 suite
# gained and changed tests. Fetched at HEAD, HOLOS 0.27.0 "regressed" on 33 tests it had
# never been run against. Moving a pin is a deliberate change: update the commit below, run
# `HOLOS_UPDATE_CONFORMANCE=1 cargo test -p holos-conformance --test w3c`, and review the
# baseline diff in the same commit.
#
# SHACL_Engine needs nothing fetched: it carries its own copy of the SHACL suites in
# shacl/testsuite/. tarka's fixtures are committed. Without these suites HOLOS's conformance
# tests skip rather than fail, so a fresh clone still tests green.
#
# (holos/scripts/fetch-testsuites.sh is HOLOS's own, which fetches rdf-tests at HEAD; it is
# left as it was so that subtree pulls from pwin/triplestore stay clean.)
set -eu
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"

RDF_TESTS_COMMIT=8af71fed933539d09d5f4658fb1ea7ba4c8e30b9    # 2026-08-21
DATA_SHAPES_COMMIT=7a293f5aed6876910a59fac22502eb2309f2357c  # 2026-08-24

# fetch <url> <dest> <commit> <sparse paths...>
fetch() {
  url=$1 dest=$2 commit=$3
  shift 3
  # Byte-exact fixtures: a Turtle literal can spell a carriage return, and Git for Windows'
  # core.autocrlf=true would rewrite every LF. Pinned off here whatever the machine says.
  if [ ! -d "$dest/.git" ]; then
    mkdir -p "$dest"
    git -C "$dest" init -q
    git -C "$dest" remote add origin "$url"
  fi
  git -C "$dest" config core.autocrlf false
  git -C "$dest" config core.eol lf
  git -C "$dest" config remote.origin.promisor true
  git -C "$dest" config remote.origin.partialclonefilter blob:none
  git -C "$dest" sparse-checkout set "$@"
  git -C "$dest" fetch -q --depth 1 --filter=blob:none origin "$commit"
  git -C "$dest" checkout -q --detach FETCH_HEAD
  echo "done: $dest at $(git -C "$dest" log -1 --format='%h %cs')"
}

fetch https://github.com/w3c/rdf-tests.git "$root/holos/testsuites/rdf-tests" \
  "$RDF_TESTS_COMMIT" rdf sparql
fetch https://github.com/w3c/data-shapes.git "$root/holos/testsuites/data-shapes" \
  "$DATA_SHAPES_COMMIT" data-shapes-test-suite shacl12-test-suite
