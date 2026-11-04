// Jenkinsfile - multi-stage CI for the Rust demo application on Zephyr.
//
//   Checkout -> Toolchain -> Build -> Lint -> Boot contract -> Image -> Package
//   -> Release (tags) -> Archive
//
// Package builds dist/<repo>-<version>.tar.gz (outputs, manifest.json,
// SHA256SUMS) on every run.  Release runs only for a tag v<version> and
// publishes dist/ as a GitHub release with the Jenkins secret-text credential
// github-release-token.  To release: bump the version, push, then push the
// tag (git tag v0.2.0 && git push origin v0.2.0).
//
// Builds run through scripts/docker-dev.sh (RZCON-001). The image chain is
// stm32-dev -> stm32-dev-zephyr -> stm32-dev-rust-zephyr; the Toolchain stage
// creates the missing layers first when the agent does not have them yet.

pipeline {
    agent any

    options {
        skipDefaultCheckout()
        timestamps()
        timeout(time: 60, unit: 'MINUTES')
        buildDiscarder(logRotator(numToKeepStr: '30', artifactNumToKeepStr: '10'))
        disableConcurrentBuilds()
    }

    environment {
        BASE_IMAGE   = 'stm32-dev:latest'
        ZEPHYR_IMAGE = 'stm32-dev-zephyr:latest'
        STM32_IMAGE  = 'stm32-dev-rust-zephyr:latest'
    }

    stages {
        stage('Checkout') {
            steps {
                checkout scm
                sh '''
                    git log -1 --pretty=fuller
                    echo "commit: $(git rev-parse HEAD)"
                    echo "branch: $(git rev-parse --abbrev-ref HEAD)"
                '''
            }
        }

        stage('Toolchain') {
            steps {
                sh '''
                    docker version --format 'docker server: {{.Server.Version}}'
                    if ! docker image inspect "${STM32_IMAGE}" >/dev/null 2>&1; then
                        echo "Building ${STM32_IMAGE} (and any missing base layers)"
                        make image
                    fi
                    docker image inspect --format 'base:   {{.Id}}' "${BASE_IMAGE}"
                    docker image inspect --format 'zephyr: {{.Id}}' "${ZEPHYR_IMAGE}"
                    docker image inspect --format 'image:  {{.Id}}' "${STM32_IMAGE}"
                    scripts/docker-dev.sh run cmake --version | head -n 1
                    scripts/docker-dev.sh run rustc --version
                    scripts/docker-dev.sh run bash -c 'echo "zephyr-lang-rust $(cat "$ZEPHYR_RUST_MODULE/REVISION")"'
                '''
            }
        }

        stage('Build') {
            steps {
                sh 'make build'
            }
        }

        stage('Lint') {
            steps {
                sh 'scripts/lint.sh'
            }
        }

        stage('Boot contract') {
            steps {
                sh '''
                    python3 - <<'PY'
from pathlib import Path

import re

# The application must stay compatible with bootloader:
# BOOT maps the active bank at 0x08000000, so every image links at the slot
# offset. These values mirror LAY-001..LAY-005, HND-003 and Appendix B.
def constants(path, pattern):
    text = Path(path).read_text(encoding="utf-8")
    return {m.group(1): int(m.group(2).replace("_", "").rstrip("ULul"), 16)
            for m in re.finditer(pattern, text)}

rust = constants("src/board.rs", r"pub const (\w+): u32 = (0x[0-9A-Fa-f_]+);")
c = constants("src/platform.c", r"#define (\w+)\s+(0x[0-9A-Fa-f]+U?L?)")

expected = [
    (rust, "src/board.rs", "SLOT_BASE", 0x0800C000),
    (rust, "src/board.rs", "CONFIRMED_MAGIC", 0x434F4E46),
    (rust, "src/board.rs", "STAY_IN_BOOT_MAGIC", 0x59415453),
    (rust, "src/board.rs", "HANDOVER_MAGIC", 0x00444E48),
    (c, "src/platform.c", "META_RUNNING_ADDR", 0x08008000),
    (c, "src/platform.c", "META_SIZE", 0x00004000),
    (c, "src/platform.c", "META_CONFIRMED_OFFSET", 0x140),
]
for table, path, name, want in expected:
    if name not in table:
        raise SystemExit(f"boot contract: {name} is missing from {path}")
    if table[name] != want:
        raise SystemExit(f"boot contract: {name} is 0x{table[name]:08X}, expected 0x{want:08X}")

settings = {}
for line in Path("prj.conf").read_text(encoding="utf-8").splitlines():
    line = line.strip()
    if not line or line.startswith("#") or "=" not in line:
        continue
    key, _, value = line.partition("=")
    settings[key.strip()] = value.strip()

expected_settings = {
    "CONFIG_FLASH_LOAD_OFFSET": "0xC000",
    "CONFIG_FLASH_LOAD_SIZE": "0xF4000",
}
for key, want in expected_settings.items():
    got = settings.get(key)
    if got != want:
        raise SystemExit(f"boot contract: {key} is {got!r}, expected {want!r}")

print("boot contract: devicetree offsets and shared constants match BOOT")
PY
                '''
            }
        }

        stage('Image') {
            steps {
                sh '''
                    make size
                    python3 - <<'PY'
import struct
import zlib
from pathlib import Path

version = {}
for line in Path("VERSION").read_text(encoding="utf-8").splitlines():
    if "=" in line:
        key, _, value = line.partition("=")
        version[key.strip()] = value.strip()
want = (int(version["VERSION_MAJOR"]) << 24) | (int(version["VERSION_MINOR"]) << 16) | int(version["PATCHLEVEL"])

path = Path("build/rust-zephyr-app.img")
data = path.read_bytes()
header = struct.unpack_from("<IHHIIIIIII", data, 0)
magic, hver, hsize, itype, ver, isize, icrc, load, entry, _ts = header
assert magic == 0x48574249, "bad image magic"
assert hver == 1, f"unexpected header version {hver}"
assert hsize == 256, f"unexpected header size {hsize}"
assert itype == 1, f"unexpected image type {itype}"
assert ver == want, f"image version 0x{ver:08X} does not match VERSION 0x{want:08X}"
assert load == 0x0800C000, f"image loads at 0x{load:08X}, expected 0x0800C000"
assert len(data) == hsize + isize, "image size does not match the header"
assert zlib.crc32(data[hsize:]) & 0xFFFFFFFF == icrc, "image CRC32 mismatch"
sp = struct.unpack_from("<I", data, hsize)[0]
assert 0x20000000 < sp <= 0x20080000, "initial stack pointer is outside internal RAM"
entry_ok = entry & 1 and 0x0800C000 <= (entry & ~1) < 0x0800C000 + 0x000F4000
assert entry_ok, "reset vector is outside the slot"
print(f"image: {path} v{(ver >> 24) & 0xFF}.{(ver >> 16) & 0xFF}.{ver & 0xFF} "
      f"({isize} bytes payload, entry 0x{entry:08X})")
PY
                '''
            }
        }

        stage('Package') {
            steps {
                sh '''
                    python3 ci/package.py --name app-rust-zephyr --version "$(ci/version.sh)" \
                        build/rust-zephyr-app.elf build/rust-zephyr-app.bin build/rust-zephyr-app.hex build/rust-zephyr-app.map build/rust-zephyr-app.img \
                        README.md LICENSE
                '''
            }
        }

        stage('Release') {
            // Only for a tag build (Multibranch job with tag discovery).  The
            // tag must be v<version>; ci/github_release.py checks it.
            when { buildingTag() }
            steps {
                withCredentials([string(credentialsId: 'github-release-token', variable: 'GITHUB_TOKEN')]) {
                    sh 'python3 ci/github_release.py --tag "${TAG_NAME}" --version "$(ci/version.sh)" dist/*'
                }
            }
        }

        stage('Archive') {
            steps {
                archiveArtifacts artifacts: 'dist/*', fingerprint: true
            }
        }
    }

    post {
        always {
            deleteDir()
        }
        success {
            echo 'Rust Zephyr application: build, lint, boot-contract and image checks passed.'
        }
        failure {
            echo 'Rust Zephyr application: pipeline failed - see the stage logs above.'
        }
    }
}
