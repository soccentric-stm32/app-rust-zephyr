# Docker Development Environment

| | |
|---|---|
| **Author** | Sandesh Ghimire <sandesh@soccentric.com> |
| **Date** | 2026-11-04 |
| **Board** | NUCLEO-F767ZI |

The image `stm32-dev-rust-zephyr:latest` extends `stm32-dev-zephyr:latest`
(the Zephyr application's image), which extends the shared `stm32-dev:latest`
image of the C projects.  OpenOCD, GDB, picocom, `arm-none-eabi-objcopy` and
the `scripts/docker-dev.sh` commands (flash, debug, serial, probe) therefore
work exactly as there.

| Layer | Adds |
|---|---|
| `stm32-dev` | Arm GCC, CMake/Ninja, OpenOCD, ST-LINK tools, STM32CubeF7 |
| `stm32-dev-zephyr` | Zephyr v4.4.2 (`hal_stm32`, CMSIS) in `/opt/zephyrproject`, its Python requirements, Zephyr SDK 1.0.1 (`arm-zephyr-eabi`) |
| `stm32-dev-rust-zephyr` | Rust 1.99.1 (`thumbv7em-none-eabi` and `-eabihf`, clippy, rustfmt) in `/opt/rustup`, libclang for bindgen, and Zephyr's Rust module `zephyr-lang-rust` at `dd73abc2` in `$ZEPHYR_RUST_MODULE` |

The `zephyr-lang-rust` revision is the one Zephyr v4.4.2 itself pins in
`submanifests/optional.yaml`; the module is used unmodified.

| File | Role |
|---|---|
| [Dockerfile](Dockerfile) | The Rust layer on top of `stm32-dev-zephyr` (build args `RUST_VERSION`, `ZEPHYR_RUST_REVISION`) |
| [Dockerfile.zephyr](Dockerfile.zephyr) | Identical copy of the Zephyr application's Dockerfile, used to build `stm32-dev-zephyr` when it is not on the machine yet |
| [Dockerfile.base](Dockerfile.base) | Identical copy of the validation firmware's Dockerfile, used to build `stm32-dev` when it is not on the machine yet |

`scripts/docker-dev.sh build` builds the missing layers in order, then this
one.  Cargo fetches crates during the build into `.cargo-home/` in the
project, so the first build needs network access.

| Variable | Default |
|---|---|
| `STM32_IMAGE` | `stm32-dev-rust-zephyr:latest` |
| `ZEPHYR_IMAGE` | `stm32-dev-zephyr:latest` |
| `BASE_IMAGE` | `stm32-dev:latest` |
