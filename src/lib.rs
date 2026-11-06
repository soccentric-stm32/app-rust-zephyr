// rust-zephyr-app - Rust demo application on Zephyr, entry point
//
// Author:  Sandesh Ghimire <sandesh@soccentric.com>
// Date:    2026-11-04
// Target:  NUCLEO-F767ZI (Zephyr board nucleo_f767zi), started by BOOT
// SRS:     SRS-RZEPHYR-F767, SRS-DEMO-F767
//
// Zephyr's main() (in the Rust module) calls rust_main().  It runs the
// reset-safe start-up sequence, then starts the four application threads
// (Health, Comms, Button, LED) and returns; Zephyr keeps the threads
// running.  The only flash write is the CONFIRMED record in boot.rs.

#![no_std]
#![deny(warnings)]

#[macro_use]
mod log;

mod board;
mod boot;
mod build_info;
mod button;
mod comms;
mod fault;
mod health;
mod hwlink;
mod identity;
mod ipc;
mod led;
mod plat;

// RZRT-001: Health highest, Comms high, Button and LED normal (the C Zephyr
// demo's priorities and stack sizes, with more stack for Rust's formatting).
pub(crate) const PRIO_HEALTH: i32 = 2;
pub(crate) const PRIO_COMMS: i32 = 3;
pub(crate) const PRIO_BUTTON: i32 = 4;
pub(crate) const PRIO_LED: i32 = 5;

pub(crate) const HEALTH_STACK: usize = 2048;
pub(crate) const COMMS_STACK: usize = 4096;
pub(crate) const BUTTON_STACK: usize = 1536;
pub(crate) const LED_STACK: usize = 1536;

#[no_mangle]
extern "C" fn rust_main() {
    // The fault record is in no-init RAM; bump its boot id first.
    plat::fault_boot_start();

    // RZBT-002: feed the watchdog BOOT started before any long set-up.
    plat::wdt_feed();

    // RZGEN-005: keep the reset cause for Status, then clear it.
    let reset_cause = boot::take_reset_cause();

    // RZBT-004: decode the hand-over record before Health starts.
    let handover = boot::read_handover();

    // RZDBG-001: start-up line on the debug UART.
    log_info!(
        "start v{} {}, slot {} {}, reset {}",
        build_info::VERSION,
        build_info::GIT_COMMIT,
        match handover.slot {
            board::SLOT_A => 'A',
            board::SLOT_B => 'B',
            _ => '?',
        },
        match handover.trial_state {
            board::TRIAL_PENDING => "on trial",
            board::TRIAL_CONFIRMED => "confirmed",
            _ => "no hand-over",
        },
        boot::reset_cause_text(reset_cause)
    );

    // RZBT-001/002: the vector table must be the image's own table.
    if plat::vtor() != board::SLOT_BASE {
        fault::fatal();
    }

    // The IWDG keeps running from BOOT even if the driver cannot take it
    // over; Health then keeps feeding it through the register path.
    let _ = plat::wdt_init();

    // LED-005: latch LD3 for the last fault or watchdog reset.
    let initial_fault = fault::last().is_some() || boot::reset_was_watchdog(reset_cause);
    if fault::last().is_some() {
        log_warn!("last fault recorded before this reset");
    }
    ipc::update_status(|s| s.fault_active = initial_fault);

    // COM-001/002: LEDs, B1 and USART3 must work before the health check.
    if !plat::gpio_init() || !plat::uart_init() {
        fault::fatal();
    }

    let info = comms::BootInfo {
        reset_cause,
        handover,
    };

    let thread = health::thread(handover);
    thread.set_priority(PRIO_HEALTH);
    thread.start();

    let thread = comms::thread(info);
    thread.set_priority(PRIO_COMMS);
    thread.start();

    let thread = button::thread();
    thread.set_priority(PRIO_BUTTON);
    thread.start();

    let thread = led::thread(initial_fault);
    thread.set_priority(PRIO_LED);
    thread.start();
}
