#!/usr/bin/env bash
#
# version.sh - print the version this build reports, without a leading v
#
# Author:  Sandesh Ghimire <sandesh@soccentric.com>
#
# Used by the Jenkins Package and Release stages.  The source of truth is
# the VERSION file (Cargo.toml must match); the release tag must be v<this version>.
#

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
awk -F' *= *' '/^VERSION_MAJOR/{a=$2} /^VERSION_MINOR/{b=$2} /^PATCHLEVEL/{c=$2} END{print a"."b"."c}' VERSION
