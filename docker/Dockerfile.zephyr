#
# Dockerfile - Zephyr RTOS build environment on top of stm32-dev
#
# Author:  Sandesh Ghimire <sandesh@soccentric.com>
# Target:  NUCLEO-F767ZI (Zephyr board nucleo_f767zi)
#
# Adds a pinned Zephyr tree (only the modules this board needs), its Python
# requirements and the Zephyr SDK Arm toolchain to the shared stm32-dev
# image, so OpenOCD, GDB and picocom work exactly as for the C projects.
# Applications are built as freestanding CMake projects against
# $ZEPHYR_BASE; west is installed for maintenance only.
#

ARG BASE_IMAGE=stm32-dev:latest
FROM ${BASE_IMAGE}

LABEL org.opencontainers.image.title="stm32-dev-zephyr" \
      org.opencontainers.image.description="STM32 Zephyr build, flash and debug environment" \
      org.opencontainers.image.authors="Sandesh Ghimire <sandesh@soccentric.com>"

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        python3-venv python3-dev device-tree-compiler gperf ccache \
        wget xz-utils file libmagic1 \
    && rm -rf /var/lib/apt/lists/*

# Python tools in a venv (Ubuntu 24.04 blocks system-wide pip installs).
ENV VIRTUAL_ENV=/opt/venv \
    PATH=/opt/venv/bin:${PATH}
RUN python3 -m venv /opt/venv && pip install --no-cache-dir west

# Zephyr at a release tag, shallow, with only the HAL and CMSIS modules.
ARG ZEPHYR_VERSION=v4.4.2
ENV ZEPHYR_BASE=/opt/zephyrproject/zephyr
RUN git clone --depth 1 --branch "${ZEPHYR_VERSION}" \
        https://github.com/zephyrproject-rtos/zephyr.git "${ZEPHYR_BASE}" \
    && cd /opt/zephyrproject \
    && west init -l zephyr \
    && west config manifest.project-filter -- '-.*,+cmsis,+cmsis_6,+hal_stm32' \
    && west update --narrow --fetch-opt=--depth=1 \
    && pip install --no-cache-dir -r zephyr/scripts/requirements-base.txt \
    && find /opt/zephyrproject -name .git -prune -exec rm -rf {} + \
    && printf '%s\n' "${ZEPHYR_VERSION}" > /opt/zephyrproject/VERSION

# Zephyr SDK, Arm toolchain only.
ARG ZEPHYR_SDK_VERSION=1.0.1
ENV ZEPHYR_SDK_INSTALL_DIR=/opt/zephyr-sdk-${ZEPHYR_SDK_VERSION} \
    ZEPHYR_TOOLCHAIN_VARIANT=zephyr
RUN cd /opt \
    && wget -q "https://github.com/zephyrproject-rtos/sdk-ng/releases/download/v${ZEPHYR_SDK_VERSION}/zephyr-sdk-${ZEPHYR_SDK_VERSION}_linux-x86_64_minimal.tar.xz" \
    && tar xf "zephyr-sdk-${ZEPHYR_SDK_VERSION}_linux-x86_64_minimal.tar.xz" \
    && rm "zephyr-sdk-${ZEPHYR_SDK_VERSION}_linux-x86_64_minimal.tar.xz" \
    && "${ZEPHYR_SDK_INSTALL_DIR}/setup.sh" -t arm-zephyr-eabi -h -c \
    && chmod -R a+rX /opt/zephyrproject "${ZEPHYR_SDK_INSTALL_DIR}" /opt/venv

LABEL org.opencontainers.image.version="zephyr-${ZEPHYR_VERSION}-sdk-${ZEPHYR_SDK_VERSION}"

WORKDIR /work
CMD ["bash"]
