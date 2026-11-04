#!/usr/bin/env bash
#
# lint.sh - rustfmt and clippy for the Rust crate (RZGEN-006)
#
# Author:  Sandesh Ghimire <sandesh@soccentric.com>
# Usage:   scripts/lint.sh             after scripts/build.sh has configured build/
#
# Clippy needs the environment Zephyr's Rust module sets up for cargo
# (DOTCONFIG, INCLUDE_DIRS, ...).  Rather than copy it, take the module's own
# cargo command from build/build.ninja and run it as "cargo clippy".  Only
# this crate is checked with -D warnings; the zephyr crates are upstream.
#

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
DEV="$ROOT/scripts/docker-dev.sh"
cd "$ROOT"

[[ -f build/build.ninja ]] || { echo "lint: run scripts/build.sh first" >&2; exit 1; }

CARGO_CMD="$(grep -o 'cmake -E env BUILD_DIR.*--target-dir /work/build/rust/target' build/build.ninja | head -n 1)"
[[ -n "$CARGO_CMD" ]] || { echo "lint: cargo command not found in build/build.ninja" >&2; exit 1; }
CLIPPY_CMD="${CARGO_CMD/ cargo build / cargo clippy }"

"$DEV" run env CARGO_HOME=/work/.cargo-home cargo fmt --check
# The crate's own checks stay strict.  The build config allows two lints
# only because the pinned zephyr/zephyr-sys path dependencies trigger them;
# re-deny them here so the application cannot hide the same diagnostics.
"$DEV" run env CARGO_HOME=/work/.cargo-home bash -c "cd /work && $CLIPPY_CMD -- -D warnings -D dead_code -D unnecessary_transmutes"
echo "lint: rustfmt and clippy clean"
