// plat.rs - Rust side of the C platform layer (src/platform.c)
//
// Author: Sandesh Ghimire <sandesh@soccentric.com>
// SRS:    RZCON-004
//
// Declarations must match src/platform.h.  Every wrapper here is safe: the C
// functions take fixed arguments and check their own bounds.

use core::ffi::{c_char, CStr};

use zephyr::time::{Duration, Tick};

pub const FAULT_TASK_LEN: usize = 16;
pub const FAULT_FILE_LEN: usize = 32;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Fault {
    pub reason: u32,
    pub cfsr: u32,
    pub hfsr: u32,
    pub mmfar: u32,
    pub bfar: u32,
    pub pc: u32,
    pub lr: u32,
    pub psr: u32,
    pub line: u32,
    pub task: [u8; FAULT_TASK_LEN],
    pub file: [u8; FAULT_FILE_LEN],
}

impl Fault {
    const fn empty() -> Self {
        Self {
            reason: 0,
            cfsr: 0,
            hfsr: 0,
            mmfar: 0,
            bfar: 0,
            pc: 0,
            lr: 0,
            psr: 0,
            line: 0,
            task: [0; FAULT_TASK_LEN],
            file: [0; FAULT_FILE_LEN],
        }
    }
}

mod ffi {
    use core::ffi::c_char;

    use super::Fault;

    extern "C" {
        pub fn plat_fault_boot_start();
        pub fn plat_fault_get(out: *mut Fault) -> bool;
        pub fn plat_fault_is_current_boot() -> bool;
        pub fn plat_fault_is_previous_boot() -> bool;
        pub fn plat_fault_clear();
        pub fn plat_fatal() -> !;

        pub fn plat_vtor() -> u32;
        pub fn plat_reboot() -> !;
        pub fn plat_dcache_invalidate(addr: usize, len: u32);
        pub fn plat_thread_name(name: *const c_char);
        pub fn plat_stack_unused() -> u32;

        pub fn plat_wdt_init() -> bool;
        pub fn plat_wdt_feed();

        pub fn plat_gpio_init() -> bool;
        pub fn plat_led_set(led: u32, on: bool);
        pub fn plat_button_pressed() -> bool;
        pub fn plat_button_wait(timeout_ms: u32);

        pub fn plat_uart_init() -> bool;
        pub fn plat_uart_set_baud(baud: u32) -> bool;
        pub fn plat_uart_read(dst: *mut u8, max_len: u32) -> u32;
        pub fn plat_uart_wait(timeout_ms: u32);
        pub fn plat_uart_rx_running() -> bool;
        pub fn plat_uart_send(data: *const u8, len: u32, timeout_ms: u32) -> bool;

        pub fn plat_confirmed_read(out32: *mut u8) -> bool;
        pub fn plat_confirmed_write(record32: *const u8) -> bool;
    }
}

// SAFETY (all wrappers below): the C functions take no pointers, or only
// pointers to buffers of the length passed with them, and are safe to call
// from any thread once the kernel runs.

pub fn fault_boot_start() {
    unsafe { ffi::plat_fault_boot_start() }
}

pub fn fault_get() -> Option<Fault> {
    let mut fault = Fault::empty();
    if unsafe { ffi::plat_fault_get(&mut fault) } {
        Some(fault)
    } else {
        None
    }
}

pub fn fault_is_current_boot() -> bool {
    unsafe { ffi::plat_fault_is_current_boot() }
}

pub fn fault_is_previous_boot() -> bool {
    unsafe { ffi::plat_fault_is_previous_boot() }
}

pub fn fault_clear() {
    unsafe { ffi::plat_fault_clear() }
}

pub fn fatal() -> ! {
    unsafe { ffi::plat_fatal() }
}

pub fn vtor() -> u32 {
    unsafe { ffi::plat_vtor() }
}

pub fn reboot() -> ! {
    unsafe { ffi::plat_reboot() }
}

pub fn dcache_invalidate(addr: u32, len: u32) {
    unsafe { ffi::plat_dcache_invalidate(addr as usize, len) }
}

pub fn thread_name(name: &'static CStr) {
    unsafe { ffi::plat_thread_name(name.as_ptr() as *const c_char) }
}

/// Unused bytes on the calling thread's stack (RZRT-005).
pub fn stack_unused() -> u32 {
    unsafe { ffi::plat_stack_unused() }
}

pub fn wdt_init() -> bool {
    unsafe { ffi::plat_wdt_init() }
}

pub fn wdt_feed() {
    unsafe { ffi::plat_wdt_feed() }
}

pub fn gpio_init() -> bool {
    unsafe { ffi::plat_gpio_init() }
}

/// `led` is 0 for LD1, 1 for LD2 and 2 for LD3.
pub fn led_set(led: u32, on: bool) {
    unsafe { ffi::plat_led_set(led, on) }
}

pub fn button_pressed() -> bool {
    unsafe { ffi::plat_button_pressed() }
}

/// Wait for a B1 edge or the timeout, whichever comes first.
pub fn button_wait(timeout_ms: u32) {
    unsafe { ffi::plat_button_wait(timeout_ms) }
}

pub fn uart_init() -> bool {
    unsafe { ffi::plat_uart_init() }
}

pub fn uart_set_baud(baud: u32) -> bool {
    unsafe { ffi::plat_uart_set_baud(baud) }
}

pub fn uart_read(dst: &mut [u8]) -> usize {
    unsafe { ffi::plat_uart_read(dst.as_mut_ptr(), dst.len() as u32) as usize }
}

/// Wait for received bytes or the timeout, whichever comes first.
pub fn uart_wait(timeout_ms: u32) {
    unsafe { ffi::plat_uart_wait(timeout_ms) }
}

pub fn uart_rx_running() -> bool {
    unsafe { ffi::plat_uart_rx_running() }
}

/// Send one frame and wait until its last stop bit has left the UART.
pub fn uart_send(data: &[u8], timeout_ms: u32) -> bool {
    unsafe { ffi::plat_uart_send(data.as_ptr(), data.len() as u32, timeout_ms) }
}

pub fn confirmed_read() -> Option<[u8; 32]> {
    let mut record = [0u8; 32];
    if unsafe { ffi::plat_confirmed_read(record.as_mut_ptr()) } {
        Some(record)
    } else {
        None
    }
}

pub fn confirmed_write(record: &[u8; 32]) -> bool {
    unsafe { ffi::plat_confirmed_write(record.as_ptr()) }
}

/// Milliseconds since boot.
pub fn now_ms() -> u64 {
    zephyr::sys::uptime_get() as u64
}

pub fn sleep_ms(ms: u32) {
    let _ = zephyr::time::sleep(Duration::millis(ms as Tick));
}
