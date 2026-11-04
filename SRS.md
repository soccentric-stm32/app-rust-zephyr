# Software Requirements Specification — NUCLEO-F767ZI Rust-on-Zephyr Demo Application

| | |
|---|---|
| **Document** | SRS-RZEPHYR-F767 |
| **Version** | 0.1 (draft) |
| **Author** | Sandesh Ghimire <sandesh@soccentric.com> |
| **Date** | 2026-11-04 |
| **Status** | Draft for review |
| **Hardware** | NUCLEO-F767ZI, STM32F767ZIT6 rev Z (see [Hardware.md](https://github.com/soccentric-stm32/validation-firmware/blob/HEAD/Hardware.md)) |
| **Software base** | Zephyr RTOS v4.4.2, Zephyr SDK 1.0.1, zephyr-lang-rust `dd73abc2`, Rust 1.99.1, board `nucleo_f767zi` |

---

## Contents

1. [Introduction](#1-introduction)
2. [Overall description](#2-overall-description)
3. [General requirements](#3-general-requirements)
4. [Bootloader contract](#4-bootloader-contract)
5. [Kernel and thread requirements](#5-kernel-and-thread-requirements)
6. [Functional requirements](#6-functional-requirements)
7. [HWLINK in Rust on Zephyr](#7-hwlink-in-rust-on-zephyr)
8. [Safety and robustness](#8-safety-and-robustness)
9. [Non-functional requirements](#9-non-functional-requirements)
10. [Verification matrix](#10-verification-matrix)
- [Appendix A — Mapping to the other demos](#appendix-a--mapping-to-the-other-demos)
- [Appendix B — Open items](#appendix-b--open-items)

---

## 1. Introduction

### 1.1 Purpose

This document specifies **RZDEMO**, the demo application of SRS-DEMO-F767
written in Rust on the Zephyr RTOS. It does the same job as the C demo: it
blinks the LEDs, reads the button, speaks HWLINK to the GUI, runs under the
A/B bootloader and confirms itself after a health check. It completes the
comparison grid: C and Rust, each on FreeRTOS and on Zephyr.

### 1.2 Scope

RZDEMO shall implement the **behaviour** of SRS-DEMO-F767 §6–§9 and its
bootloader contract (§4). Where this SRS says "as [DEMO] X", requirement X of
SRS-DEMO-F767 applies unchanged; only its implementation differs. Where it
says "as [ZDEMO] X", the requirement of the C Zephyr demo applies.

### 1.3 Definitions

| Term | Meaning |
|---|---|
| **RZDEMO** | The Rust-on-Zephyr application specified here |
| **[DEMO]** | The C FreeRTOS demo application, SRS-DEMO-F767 (functional baseline) |
| **[ZDEMO]** | The C Zephyr demo application, SRS-ZEPHYR-F767 |
| **[RDEMO]** | The Rust Embassy demo application, SRS-RUST-F767 |
| **Rust module** | Zephyr's `zephyr-lang-rust` module: the `zephyr`, `zephyr-sys` and `zephyr-build` crates and the CMake glue |
| **Platform layer** | `src/platform.c`: C functions for driver APIs the Rust module does not bind yet |
| **HWLINK** | UART framing and FlatBuffers protocol ([LINK]) |
| **BOOT** | The A/B bootloader, SRS-BOOT-F767 |

### 1.4 Requirement conventions

**shall / should / may**, IDs `<AREA>-<NNN>`, methods **T** (test),
**D** (demonstration), **I** (inspection), **A** (analysis), as in [BOOT] §1.4.

### 1.5 References

| ID | Document |
|---|---|
| [DEMO] | [app-c-freertos/SRS.md](https://github.com/soccentric-stm32/app-c-freertos/blob/HEAD/SRS.md) — SRS-DEMO-F767 |
| [ZDEMO] | [app-c-zephyr/SRS.md](https://github.com/soccentric-stm32/app-c-zephyr/blob/HEAD/SRS.md) — SRS-ZEPHYR-F767 |
| [RDEMO] | [app-rust-freertos/SRS.md](https://github.com/soccentric-stm32/app-rust-freertos/blob/HEAD/SRS.md) — SRS-RUST-F767 |
| [BOOT] | [bootloader/SRS.md](https://github.com/soccentric-stm32/bootloader/blob/HEAD/SRS.md) — SRS-BOOT-F767 |
| [GUI] | [validation-gui/SRS.md](https://github.com/soccentric-stm32/validation-gui/blob/HEAD/SRS.md) — SRS-GUI-F767 |
| [LINK] | [GUI] Appendix A — HWLINK protocol (normative) |
| [ZRUST] | Zephyr documentation, "Rust Language Support", and the zephyr-lang-rust repository |
| [RM0410] | ST, STM32F76xxx/F77xxx reference manual |

---

## 2. Overall description

### 2.1 Product perspective

RZDEMO replaces the C demo in a bootloader slot. It reports role `DEMO`; the
GUI tells the demos apart by the `HelloReply` name and version.

```
 BOOT (C) ── hand-over at 0x0800C000 ──> RZDEMO (Rust on Zephyr)
                                           rust_main()  start-up, then starts:
                                           ├─ LED thread
                                           ├─ Button thread   (GPIO interrupt wakes it)
                                           ├─ Comms thread <── HWLINK ──> GUI
                                           └─ Health thread ──> IWDG, CONFIRMED
                                           src/platform.c: UART, watchdog, reboot,
                                                           partition, fatal handler
```

### 2.2 Constraints

| ID | Requirement |
|---|---|
| RZCON-001 | RZDEMO shall be built in the project Docker image (`stm32-dev-rust-zephyr`, which extends `stm32-dev-zephyr` and the shared `stm32-dev` image) with one command (`scripts/build.sh`). |
| RZCON-002 | The Zephyr, SDK, Rust and Rust-module versions shall be pinned in the image (build arguments). The Rust module revision shall be the one the Zephyr release pins in `submanifests/optional.yaml`. Zephyr and the module shall be used unmodified; board-specific additions (overlay, Kconfig, devicetree augments) shall live in this repository. |
| RZCON-003 | RZDEMO shall be a freestanding Zephyr application built with CMake + Ninja; the application logic shall be one `no_std` Rust crate (package `rustapp`) built by `rust_cargo_application()`, without an allocator (`CONFIG_RUST_ALLOC` off). |
| RZCON-004 | C code shall be limited to the platform layer: driver APIs not yet bound by the Rust module and Zephyr hooks that must be C (`SYS_INIT`, `k_sys_fatal_error_handler`). It shall contain no application logic, take fixed arguments, and be declared for Rust in one module (`src/plat.rs`). |
| RZCON-005 | RZDEMO shall use USART3 (PD8/PD9) for HWLINK and shall disable the Zephyr console, `printk` and logging backends. |
| RZCON-006 | RZDEMO shall run only under BOOT, linked at `0x0800 C000` ([BOOT] LAY-002), and shall never reconfigure PA13/PA14 (SWD) or PH0/PH1 (HSE). |

### 2.3 Assumptions

- BOOT has verified the image, set the hand-over record and left the IWDG
  running with a 4 s timeout ([BOOT] HND-001…004).
- The board is built without the FPU, as in [ZDEMO], so the Rust target is
  `thumbv7em-none-eabi`.

---

## 3. General requirements

| ID | Requirement | Method |
|---|---|---|
| RZGEN-001 | The build shall produce `build/rust-zephyr-app.elf`, `.bin`, `.hex`, `.map` and the bootloader image file `rust-zephyr-app.img`. | I |
| RZGEN-002 | RZDEMO shall embed its version (`VERSION` file, checked against `Cargo.toml`), git commit and build date; the image header shall carry the same version, commit and timestamp (`SOURCE_DATE_EPOCH`). | I, T |
| RZGEN-003 | Clocks shall come from the board devicetree (216 MHz from HSE bypass) and the I- and D-caches shall be on, as [ZDEMO] ZGEN-003. | T |
| RZGEN-004 | CPU faults, stack overflows, `k_panic()` and Rust panics shall record the reason in no-init RAM, show the fault pattern on LD3 and reset (as [DEMO] DGEN-004). | T |
| RZGEN-005 | The reset cause shall be read from `RCC_CSR`, kept for `Status`, then cleared (as [DEMO] DGEN-005). | T |
| RZGEN-006 | The build shall have no warnings in the application: `#![deny(warnings)]` in the crate, `rustfmt --check` and `clippy -D warnings` (`scripts/lint.sh`), and `CONFIG_COMPILER_WARNINGS_AS_ERRORS=y` for the C part. | I |
| RZGEN-007 | All source files shall carry the author header of the project. | I |

---

## 4. Bootloader contract

| ID | Requirement | Method |
|---|---|---|
| RZBT-001 | Kconfig shall set `CONFIG_FLASH_LOAD_OFFSET=0xC000` and `CONFIG_FLASH_LOAD_SIZE=0xF4000` with `CONFIG_USE_DT_CODE_PARTITION=n`; the build shall check both (`BUILD_ASSERT`). | I, T |
| RZBT-002 | The IWDG started by BOOT shall be fed in `PRE_KERNEL_1` and again at the start of `rust_main()`; `rust_main()` shall check that `VTOR` is `0x0800 C000`. | T |
| RZBT-003 | Zephyr's clock initialisation shall start from the reset state BOOT leaves ([BOOT] HND-001). | T |
| RZBT-004 | RZDEMO shall read the hand-over record in `RTC_BKP2R` and report slot and trial state, as [DEMO] DBT-004. | T |
| RZBT-005 | `EnterBootloader` shall write the `STAY` request to `RTC_BKP1R` and reset with `sys_reboot(SYS_REBOOT_COLD)`, as [DEMO] DBT-005. | T |
| RZBT-006 | The only flash write shall be the `CONFIRMED` record ([BOOT] Appendix B) through the Zephyr flash API on `app_meta_partition`, which covers only the running slot's metadata sector. | I |

---

## 5. Kernel and thread requirements

| ID | Requirement | Method |
|---|---|---|
| RZRT-001 | RZDEMO shall run four static Rust threads (`#[zephyr::thread]`) with the priorities of [ZDEMO]: Health 2, Comms 3, Button 4, LED 5, and explicit stack sizes. | I |
| RZRT-002 | Threads shall share data only through static fixed-size queues and a status snapshot guarded by critical sections, and through atomics. | I |
| RZRT-003 | The kernel tick shall be 1 kHz (`CONFIG_SYS_CLOCK_TICKS_PER_SEC=1000`). | I |
| RZRT-004 | Every thread shall check in with Health at least every 500 ms; Health shall stop feeding the watchdog when a thread misses its check-in for 2 s (as [DEMO] DRT-012). Health shall be the only feeder after start-up. | T |
| RZRT-005 | Each thread shall sample its own unused stack (`k_thread_stack_space_get`) for `Status` (as [DEMO] DRT-013). | T |
| RZRT-006 | Stack protection (`CONFIG_HW_STACK_PROTECTION`, MPU) shall be enabled. | I |

---

## 6. Functional requirements

| ID | Requirement | Method |
|---|---|---|
| RZFN-001 | **LEDs:** as [DEMO] LED-001…LED-007, on the `led0`…`led2` devicetree aliases. | T, D |
| RZFN-002 | **Button:** as [DEMO] BTN-001…BTN-004, on `sw0` with a GPIO interrupt and 30 ms debounce. | T, D |
| RZFN-003 | **Communication:** as [DEMO] COM-001…COM-007, with an interrupt-driven USART3 driver (RX ring, TX completion) in the platform layer. | T |
| RZFN-004 | **Health and confirmation:** as [DEMO] HLT-001…HLT-005, with a `CONFIG_RZDEMO_FAIL_HEALTH` Kconfig option exported to Rust as a `cfg`. | T |
| RZFN-005 | `HelloReply` shall report role `DEMO`, firmware name `rust-zephyr-app` and a version text with the Rust and Zephyr versions; the detail field shall carry the factory identity (PRV1) as in [RDEMO]. | T |

---

## 7. HWLINK in Rust on Zephyr

| ID | Requirement | Method |
|---|---|---|
| RZLNK-001 | Messages shall follow the shared schema `protocol/hwlink.fbs` ([LINK] A.5), encoded with the fixed-buffer FlatBuffers codec of [RDEMO] (no allocator). | I, T |
| RZLNK-002 | COBS framing and CRC-32/ISO-HDLC shall follow [LINK] A.2. | T |
| RZLNK-003 | The framing, codec and message set should stay the same source as [RDEMO] (`src/hwlink/`), so both Rust demos behave identically. | I |
| RZLNK-004 | A repeated sequence number shall resend the cached reply without re-executing; a `Hello` shall never be answered from the cache. | T |

---

## 8. Safety and robustness

| ID | Requirement | Method |
|---|---|---|
| RZSAFE-001 | RZDEMO shall never unlock or write option bytes. | I |
| RZSAFE-002 | The confirm path shall accept only the `CONFIRMED` record address, fixed in the platform layer (as [DEMO] DSAFE-002); a record that is neither erased nor valid shall never be overwritten. | I, T |
| RZSAFE-003 | RZDEMO shall drive only PB0, PB7, PB14, read PC13 and use PD8/PD9; unused peripherals of the board shall be disabled in the overlay. | I |
| RZSAFE-004 | `CONFIG_ASSERT` and stack protection shall be on; the Rust release profile shall use `panic = "abort"`. | I |

---

## 9. Non-functional requirements

| ID | Requirement | Method |
|---|---|---|
| RZNFR-001 | **Footprint:** RZDEMO shall be under 128 KB of flash and 64 KB of RAM; the build shall print and enforce both. | I |
| RZNFR-002 | **Start-up:** the LED pattern shall be visible within 300 ms of hand-over. | T |
| RZNFR-003 | **Reliability:** as [DEMO] DNFR-003 (24 h with the GUI polling every 100 ms). | T |
| RZNFR-004 | **Comparison:** contributes the Rust-on-Zephyr column to the comparison report ([RDEMO] RNFR-004). | A |
| RZNFR-005 | **Template:** the README shall explain how to start a new Rust-on-Zephyr application from RZDEMO. | I |

---

## 10. Verification matrix

| Area | Requirements | Main method |
|---|---|---|
| Constraints | RZCON-001…006 | I |
| General | RZGEN-001…007 | T, I |
| Bootloader contract | RZBT-001…006 | T, I |
| Kernel and threads | RZRT-001…006 | T, I |
| Functional | RZFN-001…005 | T, D |
| HWLINK | RZLNK-001…004 | T, I |
| Safety | RZSAFE-001…004 | I, T |
| Non-functional | RZNFR-001…005 | T, I, A |

---

## Appendix A — Mapping to the other demos

| Concern | [ZDEMO] (C on Zephyr) | [RDEMO] (Rust on Embassy) | RZDEMO (Rust on Zephyr) |
|---|---|---|---|
| Threads / tasks | `K_THREAD_DEFINE` | `#[embassy_executor::task]` | `#[zephyr::thread]`, priority set before `start()` |
| Inter-task data | `k_msgq`, `k_event` | embassy-sync `Channel`, `Watch` | static queues and snapshot in critical sections, atomics |
| LEDs, button | `gpio_dt_spec` | embassy-stm32 GPIO, EXTI | `gpio_dt_spec` in the platform layer |
| UART | IRQ API + ring buffer | `BufferedUart` | the [ZDEMO] IRQ driver in the platform layer |
| HWLINK codec | flatcc (C) | fixed-buffer codec | the [RDEMO] fixed-buffer codec |
| Watchdog | `watchdog` driver | `IndependentWatchdog` | `watchdog` driver in the platform layer |
| Faults | `k_sys_fatal_error_handler` | cortex-m-rt exceptions | `k_sys_fatal_error_handler`; Rust panics reach it through `k_panic()` |
| Flash write | Zephyr flash API on `app_meta_partition` | embassy-stm32 flash | Zephyr flash API on `app_meta_partition` |

## Appendix B — Open items

| ID | Item | Status |
|---|---|---|
| OPEN-RZ01 | The pinned zephyr-lang-rust does not list `st,stm32-flash-controller` in its devicetree augments; `dt-rust.yaml` carries a local copy with it added. Drop the copy when the pinned revision includes it. | Open |
| OPEN-RZ02 | Move UART, watchdog and reboot from the platform layer to Rust when the Rust module binds those APIs (RZCON-004). | Open |
| OPEN-RZ03 | Bench run on the NUCLEO-F767ZI: hand-over from BOOT, trial confirm, rollback with `CONFIG_RZDEMO_FAIL_HEALTH`, GUI session at 921600. | Open |
| OPEN-RZ04 | The upstream `zephyr` and `zephyr-sys` crates build with warnings (unused function, unnecessary transmutes in generated bindings); they are outside this repository's lint. | Open |
