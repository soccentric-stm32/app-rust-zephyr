// fault.rs - no-init fault record as seen from Rust
//
// Author: Sandesh Ghimire <sandesh@soccentric.com>
// SRS:    RZGEN-004, RZSAFE-004
//
// Zephyr's fatal error handler (k_sys_fatal_error_handler in platform.c)
// saves the record for CPU faults, stack overflows, k_panic() and Rust
// panics; the zephyr crate's panic handler ends in k_panic().  Rust only
// reads and clears it.

use crate::platform::plat::{self, Fault};

/// Reason names, in the order of the C demo's app_fault_reason_t.
const REASONS: [&str; 9] = [
    "none",
    "hard_fault",
    "mem_manage",
    "bus_fault",
    "usage_fault",
    "assert",
    "stack_overflow",
    "malloc_failed",
    "startup",
];

pub fn last() -> Option<Fault> {
    plat::fault_get()
}

pub fn describe(fault: &Fault) -> &'static str {
    REASONS
        .get(fault.reason as usize)
        .copied()
        .unwrap_or("unknown")
}

/// A fault saved earlier in this boot (the record survives the reset).
pub fn in_current_boot() -> bool {
    plat::fault_is_current_boot()
}

/// A fault saved in the boot before this one: a failed previous trial.
pub fn in_previous_boot() -> bool {
    plat::fault_is_previous_boot()
}

pub fn clear() {
    plat::fault_clear();
}

/// Record a fault through Zephyr's fatal path, show it on LD3 and reset.
pub fn fatal() -> ! {
    plat::fatal()
}

/// The NUL-terminated text fields of the record as `&str`.
pub fn text(field: &[u8]) -> &str {
    let len = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    core::str::from_utf8(&field[..len]).unwrap_or("")
}
