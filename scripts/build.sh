#!/usr/bin/env bash
#
# build.sh - Build the Rust-on-Zephyr demo application inside the
#            stm32-dev-rust-zephyr container (RZCON-001)
#
# Author:  Sandesh Ghimire <sandesh@soccentric.com>
# Usage:   scripts/build.sh            configure (if needed) + build
#          scripts/build.sh clean      rm -rf build, then configure + build
#
# Outputs: build/rust-zephyr-app.elf, .bin, .hex, .map and the bootloader
#          image build/rust-zephyr-app.img
#
# CMake drives the whole build; Zephyr's Rust module runs cargo for the
# crate.  The crate cache lives in .cargo-home/ (git-ignored) so it survives
# between container runs and belongs to the calling user.
#

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
DEV="$ROOT/scripts/docker-dev.sh"
cd "$ROOT"

if [[ "${1:-}" == "clean" ]]; then
    rm -rf build
fi

# RZGEN-002: one timestamp for the firmware build info and the BOOT image
# header.  SOURCE_DATE_EPOCH also makes the build reproducible.
STAMP="${SOURCE_DATE_EPOCH:-$(date +%s)}"
ENV=(env CARGO_HOME=/work/.cargo-home SOURCE_DATE_EPOCH="$STAMP")

"$DEV" run "${ENV[@]}" cmake -S /work -B /work/build -G Ninja \
    -DUSER_CACHE_DIR=/work/build/.zephyr-cache  # $ZEPHYR_BASE is read-only
"$DEV" run "${ENV[@]}" cmake --build /work/build
"$DEV" run bash -c '
    set -e
    for f in elf bin hex map img; do
        test -f "build/rust-zephyr-app.$f"
    done
    ls -l build/rust-zephyr-app.*'
