// boot.rs - bootloader hand-over, backup registers and CONFIRMED record
//
// Author: Sandesh Ghimire <sandesh@soccentric.com>
// SRS:    RZBT-004..006, RZGEN-005, RZFN-004, RZSAFE-002

use core::sync::atomic::{AtomicBool, Ordering};

use crate::board;
use crate::hwlink::frame::crc32;
use crate::plat;

#[derive(Clone, Copy)]
pub struct Handover {
    pub slot: u8,
    pub trial_state: u8,
    pub trial_count: u8,
}

impl Handover {
    pub const fn unknown() -> Self {
        Self {
            slot: board::SLOT_UNKNOWN,
            trial_state: board::TRIAL_UNKNOWN,
            trial_count: 0,
        }
    }

    pub fn pending(&self) -> bool {
        self.trial_state == board::TRIAL_PENDING
    }

    /// Trial state to report now.  The hand-over is decoded once at start-up,
    /// so a slot that was on trial reports CONFIRMED once the health thread
    /// has written the CONFIRMED record.
    pub fn current_trial_state(&self) -> u8 {
        if self.pending() && CONFIRMED.load(Ordering::Acquire) {
            board::TRIAL_CONFIRMED
        } else {
            self.trial_state
        }
    }
}

/// Set once the CONFIRMED record for the running slot is in flash.
static CONFIRMED: AtomicBool = AtomicBool::new(false);

/// Record that the running slot is no longer on trial.
pub fn mark_confirmed() {
    CONFIRMED.store(true, Ordering::Release);
}

/// RZGEN-005: save the reset flags for `Status`, then clear them.
pub fn take_reset_cause() -> u32 {
    // SAFETY: RCC_CSR is a read/write register; this is its only user (the
    // Zephyr hwinfo driver is not enabled).
    unsafe {
        let csr = board::RCC_CSR.read_volatile();
        board::RCC_CSR.write_volatile(csr | board::RCC_CSR_RMVF);
        csr & board::RCC_CSR_RESET_MASK
    }
}

pub fn reset_was_watchdog(cause: u32) -> bool {
    cause & (board::RCC_CSR_WDGRSTF | board::RCC_CSR_WWDGRSTF) != 0
}

/// Text used by `Status`; the order matches the Rust Embassy demo.
pub fn reset_cause_text(cause: u32) -> &'static str {
    if cause & board::RCC_CSR_WDGRSTF != 0 {
        "independent watchdog"
    } else if cause & board::RCC_CSR_WWDGRSTF != 0 {
        "window watchdog"
    } else if cause & board::RCC_CSR_SFTRSTF != 0 {
        "software"
    } else if cause & board::RCC_CSR_LPWRRSTF != 0 {
        "low power"
    } else if cause & board::RCC_CSR_BORRSTF != 0 {
        "brown-out"
    } else if cause & board::RCC_CSR_PINRSTF != 0 {
        "reset pin"
    } else if cause & board::RCC_CSR_PORRSTF != 0 {
        "power-on"
    } else {
        "unknown"
    }
}

fn backup_unlock() {
    // SAFETY: fixed RCC/PWR/RTC registers of the STM32F767.  The backup domain
    // is enabled and the RTC write protection opened for the single access
    // that follows; no option bytes are touched (RZSAFE-001).  Zephyr does not
    // offer a backup-register API for the F7, so the C demo does the same.
    unsafe {
        board::RCC_APB1ENR.write_volatile(
            board::RCC_APB1ENR.read_volatile()
                | board::RCC_APB1ENR_PWREN
                | board::RCC_APB1ENR_RTCEN,
        );
        board::PWR_CR.write_volatile(board::PWR_CR.read_volatile() | board::PWR_CR_DBP);
        board::RTC_WPR.write_volatile(board::RTC_WPR_UNLOCK1);
        board::RTC_WPR.write_volatile(board::RTC_WPR_UNLOCK2);
    }
}

fn backup_lock() {
    // SAFETY: fixed RTC write-protect register; re-locking is always valid.
    unsafe { board::RTC_WPR.write_volatile(board::RTC_WPR_LOCK) };
}

/// RZBT-004: decode the hand-over record written by BOOT in RTC_BKP2R.
pub fn read_handover() -> Handover {
    backup_unlock();
    // SAFETY: fixed RTC backup register; the domain is unlocked above.
    let word = unsafe { board::RTC_BKP2R.read_volatile() };
    backup_lock();
    if word & board::HANDOVER_MAGIC_MASK != board::HANDOVER_MAGIC {
        return Handover::unknown();
    }
    let slot = ((word >> board::HANDOVER_SLOT_SHIFT) & board::HANDOVER_SLOT_MASK) as u8;
    if slot != board::SLOT_A && slot != board::SLOT_B {
        return Handover::unknown();
    }
    Handover {
        slot,
        trial_state: if word & (1 << board::HANDOVER_PENDING_BIT) != 0 {
            board::TRIAL_PENDING
        } else {
            board::TRIAL_CONFIRMED
        },
        trial_count: ((word >> board::HANDOVER_TRIAL_SHIFT) & board::HANDOVER_TRIAL_MASK) as u8,
    }
}

/// RZBT-005: leave a request for BOOT and reset.
pub fn enter_bootloader() -> ! {
    log_info!("HWLINK: EnterBootloader, resetting into BOOT");
    backup_unlock();
    // SAFETY: fixed RTC backup register; the domain is unlocked above.
    unsafe { board::RTC_BKP1R.write_volatile(board::STAY_IN_BOOT_MAGIC) };
    backup_lock();
    plat::reboot()
}

pub fn reset() -> ! {
    log_info!("HWLINK: Reset");
    plat::reboot()
}

/// RZBT-006 / RZSAFE-002: the only flash write in RZDEMO.  The record
/// address is fixed in platform.c; nothing here can name another one.
pub fn write_confirmed() -> bool {
    let Some(current) = plat::confirmed_read() else {
        return false;
    };
    if confirmed_valid(&current) {
        // HLT-005: an already confirmed slot does not write flash again.
        return true;
    }
    if current.iter().any(|&b| b != 0xFF) {
        // Append-only: a torn or foreign record is never overwritten.
        return false;
    }

    let mut record = [0u8; 32];
    record[0..4].copy_from_slice(&board::CONFIRMED_MAGIC.to_le_bytes());
    let crc = crc32(&record[..28]);
    record[28..32].copy_from_slice(&crc.to_le_bytes());
    if !plat::confirmed_write(&record) {
        return false;
    }

    // Read the record back from flash, not from the program buffer.
    plat::confirmed_read().is_some_and(|verify| confirmed_valid(&verify))
}

fn confirmed_valid(record: &[u8; 32]) -> bool {
    let magic = u32::from_le_bytes([record[0], record[1], record[2], record[3]]);
    if magic != board::CONFIRMED_MAGIC {
        return false;
    }
    let stored = u32::from_le_bytes([record[28], record[29], record[30], record[31]]);
    crc32(&record[..28]) == stored
}

pub fn device_id() -> (u32, u32) {
    // SAFETY: read-only DBGMCU identity register.
    let id = unsafe { board::DBGMCU_IDCODE.read_volatile() };
    let dev = id & 0x0FFF;
    let rev = (id >> 16) & 0xFFFF;
    (if dev == 0 { board::DBGMCU_DEV_ID } else { dev }, rev)
}

/// Same 64-bit value as the C demo: UID word 0 in the high half, word 1 low.
pub fn device_uid() -> u64 {
    // SAFETY: read-only factory UID words in system memory.
    unsafe {
        let w0 = board::UID_BASE.read_volatile() as u64;
        let w1 = board::UID_BASE.add(1).read_volatile() as u64;
        (w0 << 32) | w1
    }
}
