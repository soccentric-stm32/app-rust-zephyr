#
# CMakeLists.txt - Rust demo application on Zephyr (RZDEMO), NUCLEO-F767ZI
#
# Author:  Sandesh Ghimire <sandesh@soccentric.com>
# Date:    2026-11-04
# SRS:     SRS-RZEPHYR-F767 (RZCON-001..004, RZGEN-001..006)
#
# Freestanding Zephyr application built against $ZEPHYR_BASE in the
# stm32-dev-rust-zephyr container, without a west workspace.  The
# application is the Rust crate in this directory (Cargo.toml, src/*.rs),
# built by Zephyr's Rust module; src/platform.c is the small C layer for the
# driver APIs the Rust bindings do not cover yet.
#   scripts/build.sh [clean]
#

cmake_minimum_required(VERSION 3.20.0)

set(BOARD nucleo_f767zi)
set(DTC_OVERLAY_FILE "${CMAKE_CURRENT_SOURCE_DIR}/app.overlay")

# RZCON-002: Zephyr's Rust module is not part of the image's west filter, so
# add it explicitly (docker/Dockerfile installs it).
if(NOT DEFINED ENV{ZEPHYR_RUST_MODULE})
    message(FATAL_ERROR "ZEPHYR_RUST_MODULE is not set; build in the stm32-dev-rust-zephyr container")
endif()
list(APPEND ZEPHYR_EXTRA_MODULES "$ENV{ZEPHYR_RUST_MODULE}")

# --- Version and build metadata (RZGEN-002) --------------------------------
file(STRINGS "${CMAKE_CURRENT_SOURCE_DIR}/VERSION" VERSION_LINES)
foreach(_line IN LISTS VERSION_LINES)
    if(_line MATCHES "^VERSION_MAJOR[ \t]*=[ \t]*([0-9]+)")
        set(APP_VERSION_MAJOR "${CMAKE_MATCH_1}")
    elseif(_line MATCHES "^VERSION_MINOR[ \t]*=[ \t]*([0-9]+)")
        set(APP_VERSION_MINOR "${CMAKE_MATCH_1}")
    elseif(_line MATCHES "^PATCHLEVEL[ \t]*=[ \t]*([0-9]+)")
        set(APP_VERSION_PATCH "${CMAKE_MATCH_1}")
    endif()
endforeach()
if(NOT DEFINED APP_VERSION_MAJOR)
    set(APP_VERSION_MAJOR 0)
endif()
if(NOT DEFINED APP_VERSION_MINOR)
    set(APP_VERSION_MINOR 1)
endif()
if(NOT DEFINED APP_VERSION_PATCH)
    set(APP_VERSION_PATCH 0)
endif()
set(APP_VERSION "${APP_VERSION_MAJOR}.${APP_VERSION_MINOR}.${APP_VERSION_PATCH}")
set(APP_FIRMWARE_NAME "rust-zephyr-app")

execute_process(COMMAND git -C "${CMAKE_CURRENT_SOURCE_DIR}" rev-parse HEAD
    OUTPUT_VARIABLE APP_GIT_COMMIT_FULL OUTPUT_STRIP_TRAILING_WHITESPACE ERROR_QUIET)
if(DEFINED ENV{SOURCE_DATE_EPOCH})
    set(APP_BUILD_TIMESTAMP "$ENV{SOURCE_DATE_EPOCH}")
else()
    string(TIMESTAMP APP_BUILD_TIMESTAMP "%s" UTC)
endif()

# The image's Zephyr tree has no .git; its version is recorded in
# /opt/zephyrproject/VERSION by docker/Dockerfile.zephyr.
if(NOT DEFINED BUILD_VERSION AND EXISTS "$ENV{ZEPHYR_BASE}/../VERSION")
    file(STRINGS "$ENV{ZEPHYR_BASE}/../VERSION" BUILD_VERSION LIMIT_COUNT 1)
endif()

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(rust_zephyr_app LANGUAGES C)

# RZCON-003: the Rust crate (package "rustapp") becomes librustapp.a and
# provides rust_main(), called from the module's main.c.  dt-rust.yaml is a
# local copy of the module's devicetree augments with the STM32 flash
# controller added.
set(DT_AUGMENTS "${CMAKE_CURRENT_SOURCE_DIR}/dt-rust.yaml" CACHE INTERNAL "")
rust_cargo_application()

target_sources(app PRIVATE src/platform.c src/debug_log.c)
target_include_directories(app PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/src")

# --- Build products (RZGEN-001, RZGEN-002) ----------------------------------
find_package(Python3 COMPONENTS Interpreter REQUIRED)
get_filename_component(_compiler_dir "${CMAKE_C_COMPILER}" DIRECTORY)
find_program(SIZE_TOOL NAMES arm-zephyr-eabi-size arm-none-eabi-size
    HINTS "${_compiler_dir}" REQUIRED)

add_custom_target(rust_zephyr_app_outputs ALL
    COMMAND ${CMAKE_COMMAND} -E copy
            "${CMAKE_BINARY_DIR}/zephyr/zephyr.elf"
            "${CMAKE_BINARY_DIR}/rust-zephyr-app.elf"
    COMMAND ${CMAKE_COMMAND} -E copy
            "${CMAKE_BINARY_DIR}/zephyr/zephyr.bin"
            "${CMAKE_BINARY_DIR}/rust-zephyr-app.bin"
    COMMAND ${CMAKE_COMMAND} -E copy
            "${CMAKE_BINARY_DIR}/zephyr/zephyr.hex"
            "${CMAKE_BINARY_DIR}/rust-zephyr-app.hex"
    COMMAND ${CMAKE_COMMAND} -E copy
            "${CMAKE_BINARY_DIR}/zephyr/zephyr.map"
            "${CMAKE_BINARY_DIR}/rust-zephyr-app.map"
    COMMAND ${Python3_EXECUTABLE} "${CMAKE_CURRENT_SOURCE_DIR}/scripts/mkimage.py"
            --input "${CMAKE_BINARY_DIR}/rust-zephyr-app.bin"
            --output "${CMAKE_BINARY_DIR}/rust-zephyr-app.img"
            --version "${APP_VERSION}"
            --commit "${APP_GIT_COMMIT_FULL}"
            --timestamp "${APP_BUILD_TIMESTAMP}"
            --name "${APP_FIRMWARE_NAME}"
    COMMAND ${Python3_EXECUTABLE} "${CMAKE_CURRENT_SOURCE_DIR}/scripts/check_footprint.py"
            --size-tool "${SIZE_TOOL}"
            --max-flash 131072
            --max-ram 65536
            "${CMAKE_BINARY_DIR}/rust-zephyr-app.elf"
    DEPENDS zephyr_final
    BYPRODUCTS
        "${CMAKE_BINARY_DIR}/rust-zephyr-app.elf"
        "${CMAKE_BINARY_DIR}/rust-zephyr-app.bin"
        "${CMAKE_BINARY_DIR}/rust-zephyr-app.hex"
        "${CMAKE_BINARY_DIR}/rust-zephyr-app.map"
        "${CMAKE_BINARY_DIR}/rust-zephyr-app.img"
    COMMENT "Copying rust-zephyr-app.* and generating the BOOT image file")
