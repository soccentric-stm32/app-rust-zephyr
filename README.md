# app-rust-zephyr

The demo application of the NUCLEO-F767ZI system written in Rust on Zephyr RTOS (board `nucleo_f767zi`), using Zephyr's Rust module. It runs under the A/B bootloader from either slot and does the same job as the C FreeRTOS demo, so the C and Rust builds on FreeRTOS and Zephyr can be compared like for like.

| | |
|---|---|
| Requirements | [SRS.md](SRS.md) — SRS-RZEPHYR-F767 (v0.1 draft) |
| Functional baseline | [SRS-DEMO-F767](https://github.com/soccentric-stm32/app-c-freertos/blob/HEAD/SRS.md) — same behaviour as the C FreeRTOS demo |
| Board | NUCLEO-F767ZI — see [Hardware.md](https://github.com/soccentric-stm32/validation-firmware/blob/HEAD/Hardware.md) |
| Status | Builds and passes the image and boot-contract checks: bootloader slot and hand-over record, LED patterns on LD1/LD2/LD3, B1 debounce and button events, HWLINK on USART3 (Hello/Ping/Status/SetLed/SetLedAll/ClearFault/SetBaud/EnterBootloader/Reset), health check with IWDG and CONFIRMED record, fatal-error record, and the `rust-zephyr-app.img` bootloader image. Not yet run on the board. |
| Build environment | [docker/README.md](docker/README.md): `stm32-dev-rust-zephyr` = `stm32-dev-zephyr` (Zephyr v4.4.2, SDK 1.0.1) + Rust 1.99.1 + `zephyr-lang-rust` |

## Build and flash

Everything runs in the container through `scripts/docker-dev.sh`:

```bash
scripts/docker-dev.sh build   # build the image once (and the missing base images)
scripts/build.sh              # build inside the container
make lint                     # rustfmt --check and clippy -D warnings
make flash                    # upload build/rust-zephyr-app.img through BOOT (bootctl)
make erase-all                # erase BOOT + slots (asks for confirmation)
make help                     # all targets
```

Outputs in `build/`: `rust-zephyr-app.elf`, `.bin`, `.hex`, `.map` and the
BOOT image file `rust-zephyr-app.img` (256-byte image header + binary). The
image is linked at `0x0800C000` (`CONFIG_FLASH_LOAD_OFFSET`) and starts only
through the bootloader. `make flash` uploads the `.img` with
`bootctl.py update --activate`; never program the raw `.elf`/`.bin` into a
slot, because BOOT requires the header and metadata records. Point `BOOTCTL`
or `PORT` elsewhere if the defaults do not fit. The build enforces the
128 KB flash / 64 KB RAM budget and prints both figures (about 45 KB flash and
35 KB RAM today).

It is a freestanding Zephyr app: plain CMake against `$ZEPHYR_BASE`, no west
workspace needed. CMake adds Zephyr's Rust module with
`ZEPHYR_EXTRA_MODULES`, and the module runs `cargo` for the crate in this
directory, so the first build needs network access to fetch crates (cached in
`.cargo-home/`). The flash layout in [app.overlay](app.overlay) is the C
Zephyr demo's: 32 KB bootloader, the 16 KB metadata sector at `0x08008000`
(`app_meta_partition`, the only flash the application writes) and the 976 KB
slot from `0x0800C000`.

## How it is built

| Part | Language | Files |
|---|---|---|
| Application: the four threads, LED patterns, button, HWLINK framing and messages, health check, CONFIRMED record logic, hand-over and reset cause, factory identity | Rust (`no_std`, no allocator) | [src/lib.rs](src/lib.rs) and the other `src/*.rs` |
| Kernel, GPIO and flash bindings, `#[zephyr::thread]` | Rust (zephyr crate) | `$ZEPHYR_RUST_MODULE` |
| Driver glue the bindings do not cover yet: interrupt-driven USART3, watchdog, `sys_reboot`, the CONFIRMED partition, the fatal-error handler and its no-init record | C | [src/platform.c](src/platform.c), [src/platform.h](src/platform.h), declared for Rust in [src/plat.rs](src/plat.rs) |

The HWLINK codec (`src/hwlink/`) and the factory identity reader are the
same code as the Rust Embassy demo. The C layer has no application logic: it
mirrors the C Zephyr demo's `uart_hwlink.c`, `iwdg.c`, `fatal.c` and
`flash_confirm.c`, so all three Zephyr-facing parts behave the same.
[dt-rust.yaml](dt-rust.yaml) is a local copy of the module's devicetree
augments with the STM32 flash controller added; the pinned module revision
does not list it.

## What it does

- **LEDs** — LD1 green, LD2 blue and LD3 red. LD1 boots with the `HEARTBEAT`
  pattern. The GUI can set `OFF`, `ON`, `BLINK`, `HEARTBEAT`, `FAST`, `CHASE`
  and `MORSE_SOS` with a 50–5000 ms period in 10 ms steps. LD3 is reserved for
  errors and needs `force` to be set by `SetLed`.
- **Button** — B1 uses the GPIO interrupt with 30 ms debounce. A short press
  cycles the LD1+LD2 pattern; a long press (≥ 2 s) restores the defaults. Each
  press sends an unsolicited `ButtonEvent` when the HWLINK link is open.
- **HWLINK** — USART3 at 115200 8N1 by default, with an interrupt-driven RX
  ring and interrupt-driven TX. `SetBaud` switches to 921600 and reverts to
  115200 after 5 s without a valid frame. Malformed frames get
  `Nak MALFORMED`, unknown requests get `Nak UNSUPPORTED`, and a repeated
  sequence number resends the last reply.
- **Health** — the Health thread is the only IWDG feeder. All other threads
  check in at least every 500 ms; a 2 s miss stops the watchdog. A pending
  slot runs the health check 5 s after start (thread check-ins, no fault in
  this boot or the previous trial, UART receiver running) and writes the
  `CONFIRMED` record into the metadata sector.
- **Fail-health build** — set `CONFIG_RZDEMO_FAIL_HEALTH=y` (or pass
  `-DCONFIG_RZDEMO_FAIL_HEALTH=y` to CMake) to force the health check to fail
  and exercise bootloader rollback. `build.rs` exports it to Rust as
  `cfg(CONFIG_RZDEMO_FAIL_HEALTH)`.
- **Diagnostics** — CPU faults, stack overflows, `k_panic()` and Rust panics
  (the zephyr crate's panic handler ends in `k_panic()`) save
  CFSR/HFSR/MMFAR/BFAR/PC/LR and the thread name into no-init RAM, flash the
  LD3 fault pattern and reset. `Status` reports the saved fault, reset cause,
  hand-over slot/trial state, LED state and per-thread stack high-water marks.

## Starting a new application

1. Copy this repository and rename `rust-zephyr-app` in `CMakeLists.txt`,
   `src/comms.rs` and the scripts. Keep the Cargo package name `rustapp`;
   the Rust module links `librustapp.a`.
2. Keep `CONFIG_FLASH_LOAD_OFFSET=0xC000`, `CONFIG_FLASH_LOAD_SIZE=0xF4000`,
   `CONFIG_USE_DT_CODE_PARTITION=n` and the `app.overlay` flash layout. The
   application will not start from the BOOT slot without them.
3. Keep the hand-over code in `src/boot.rs` and the CONFIRMED record write
   in `src/boot.rs` and `src/platform.c`; they are the application side of
   the bootloader contract.
4. Add threads with `#[zephyr::thread(stack_size = ...)]`, set their
   priority before `start()`, and call `ipc::check_in()` from every thread
   that must keep the watchdog alive.
5. Keep USART3 for HWLINK unless the bootloader and GUI are changed together,
   and do not enable the Zephyr console, log or `printk` on it.
6. Move code from `src/platform.c` to Rust when the Rust module gains
   bindings for it; keep `src/platform.h` and `src/plat.rs` in step until then.

## Related projects

| Repository | Role |
|---|---|
| [bootloader](https://github.com/soccentric-stm32/bootloader) | A/B bootloader (UART update, CRC32, trial boot and rollback) |
| [app-c-freertos](https://github.com/soccentric-stm32/app-c-freertos) | The demo in C on FreeRTOS |
| [app-c-zephyr](https://github.com/soccentric-stm32/app-c-zephyr) | The demo in C on Zephyr RTOS |
| [app-rust-freertos](https://github.com/soccentric-stm32/app-rust-freertos) | The demo in Rust (Embassy) |
| [validation-firmware](https://github.com/soccentric-stm32/validation-firmware) | HWVAL hardware validation firmware |
| [validation-gui](https://github.com/soccentric-stm32/validation-gui) | PySide6 GUI: runs HWVAL tests, updates and flashes firmware |
| [factory-station](https://github.com/soccentric-stm32/factory-station) | HWVAL Factory: end-of-line test and provisioning station |

## License

[MIT](LICENSE) © 2026 Sandesh Ghimire. Use it as a starting point for your own
products. Third-party code (Zephyr, zephyr-lang-rust, crates.io crates and
others) is fetched at build time and keeps its own licence; `dt-rust.yaml` is
derived from zephyr-lang-rust (Apache-2.0).

Author: Sandesh Ghimire <sandesh@soccentric.com>
