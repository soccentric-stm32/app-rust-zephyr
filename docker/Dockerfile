#
# Dockerfile - Rust on Zephyr build environment on top of stm32-dev-zephyr
#
# Author:  Sandesh Ghimire <sandesh@soccentric.com>
# Target:  NUCLEO-F767ZI (Zephyr board nucleo_f767zi, Cortex-M7 with FPU)
#
# Adds Zephyr's Rust language module (zephyr-lang-rust) at the revision
# pinned by the Zephyr release in the base image, a pinned Rust toolchain
# with the thumbv7em-none-eabi target, and libclang for the bindgen step
# of zephyr-sys.  OpenOCD, GDB, picocom and the Zephyr SDK come from the
# base images, so scripts/docker-dev.sh works exactly as for the C projects.
#

ARG BASE_IMAGE=stm32-dev-zephyr:latest
FROM ${BASE_IMAGE}

LABEL org.opencontainers.image.title="stm32-dev-rust-zephyr" \
      org.opencontainers.image.description="STM32 Rust-on-Zephyr build, flash and debug environment" \
      org.opencontainers.image.authors="Sandesh Ghimire <sandesh@soccentric.com>"

ARG DEBIAN_FRONTEND=noninteractive
# gcc + libc6-dev: host linker for Cargo build scripts and proc macros.
# libclang-dev: bindgen generates the zephyr-sys bindings at build time.
RUN apt-get update \
    && apt-get install -y --no-install-recommends curl gcc libc6-dev libclang-dev \
    && rm -rf /var/lib/apt/lists/*

# Zephyr builds nucleo_f767zi without the FPU (no CONFIG_FP_HARDABI), as in
# the C Zephyr demo, so its Rust module selects thumbv7em-none-eabi; the
# hard-float target is kept for parity with the Embassy image.
#
# Toolchain in /opt, read-only at run time. Builds set CARGO_HOME to a
# directory in the project (scripts/build.sh) so the crate cache survives
# between container runs and is owned by the calling user.
ARG RUST_VERSION=1.99.1
ENV RUSTUP_HOME=/opt/rustup \
    PATH=/opt/cargo/bin:${PATH}
RUN curl -sSf https://sh.rustup.rs \
      | CARGO_HOME=/opt/cargo sh -s -- -y --no-modify-path --profile minimal \
          --default-toolchain "${RUST_VERSION}" \
          --target thumbv7em-none-eabi --target thumbv7em-none-eabihf \
          --component clippy --component rustfmt \
    && chmod -R a+rX /opt/rustup /opt/cargo \
    && printf '%s\n' "${RUST_VERSION}" > /opt/rustup/VERSION

# zephyr-lang-rust at the revision listed in Zephyr's submanifests/optional.yaml
# for this Zephyr release.  The application adds it with ZEPHYR_EXTRA_MODULES.
ARG ZEPHYR_RUST_REVISION=dd73abc242e995784da62352fe8c70d9a6c7ac2e
ENV ZEPHYR_RUST_MODULE=/opt/zephyrproject/modules/lang/rust
RUN git clone --filter=blob:none \
        https://github.com/zephyrproject-rtos/zephyr-lang-rust.git "${ZEPHYR_RUST_MODULE}" \
    && git -C "${ZEPHYR_RUST_MODULE}" checkout --quiet "${ZEPHYR_RUST_REVISION}" \
    && rm -rf "${ZEPHYR_RUST_MODULE}/.git" \
    && printf '%s\n' "${ZEPHYR_RUST_REVISION}" > "${ZEPHYR_RUST_MODULE}/REVISION" \
    && chmod -R a+rX "${ZEPHYR_RUST_MODULE}"

LABEL org.opencontainers.image.version="rust-${RUST_VERSION}-lang-rust-${ZEPHYR_RUST_REVISION}"

WORKDIR /work
CMD ["bash"]
