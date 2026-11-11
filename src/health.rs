// health.rs - health thread, IWDG refresh and CONFIRMED record
//
// Author: Sandesh Ghimire <sandesh@soccentric.com>
// SRS:    RZRT-004, RZFN-004; HLT-001..HLT-005
//
// Health is the only thread that feeds the watchdog.  Every other thread
// checks in at least every 500 ms; a 2 s gap stops the feeding, and BOOT
// counts a failed trial.  A pending slot is confirmed 5 s after start when
// every thread has checked in, no fault was saved in this boot or in the
// previous trial, and the USART3 receiver is running.

use crate::board;
use crate::boot::{self, Handover};
use crate::fault;
use crate::ipc::{self, LedCommand};
use crate::plat;

const LOOP_MS: u32 = 100;
const CHECKIN_TIMEOUT_MS: u64 = 2000;
const HEALTH_PASS_MS: u64 = 5000;
const HEALTH_LIMIT_MS: u64 = 10000;
const MIN_CHECKINS: u32 = 5;

fn set_health(state: u8, failed: bool, checks: u32) {
    ipc::update_status(|s| {
        s.health_state = state;
        s.health_failed = failed;
        s.task_checks = checks;
    });
}

fn conditions_met(handover: &Handover, counts: &[u32; ipc::TASK_COUNT]) -> bool {
    if cfg!(CONFIG_RZDEMO_FAIL_HEALTH) {
        return false;
    }
    if counts.iter().any(|&c| c < MIN_CHECKINS) {
        return false;
    }
    if fault::in_current_boot() {
        return false;
    }
    // A fault in the previous trial boot is a failed trial; an older fault
    // from before the update is only reported in Status.
    if handover.trial_count > 1 && fault::in_previous_boot() {
        return false;
    }
    plat::uart_rx_running()
}

#[zephyr::thread(stack_size = crate::HEALTH_STACK)]
pub fn thread(handover: Handover) {
    plat::thread_name(c"Health");

    let start = plat::now_ms();
    let mut last_seen = [start; ipc::TASK_COUNT];
    let mut counts = [0u32; ipc::TASK_COUNT];
    let mut check_done = false;
    let mut failed = false;
    let mut supervision_lost = false;
    let pending = handover.pending();

    plat::wdt_feed();
    set_health(
        if pending {
            board::HEALTH_RUNNING
        } else {
            board::HEALTH_NOT_REQUIRED
        },
        false,
        0,
    );

    let mut report = crate::log::TaskReport::new(
        3,
        "Health",
        "watches thread check-ins, kicks the watchdog, confirms the slot",
    );
    loop {
        report.tick();
        // Health is a thread too: its own check-in is the loop itself.
        ipc::check_in(ipc::TASK_HEALTH);

        let now = plat::now_ms();
        for task in 0..ipc::TASK_COUNT {
            let new_checks = ipc::take_check_in(task);
            if new_checks > 0 {
                counts[task] = counts[task].saturating_add(new_checks);
                last_seen[task] = now;
            }
        }
        let checks = counts.iter().sum();

        if !supervision_lost
            && last_seen
                .iter()
                .any(|&seen| now.saturating_sub(seen) > CHECKIN_TIMEOUT_MS)
        {
            // RZRT-004: stop feeding when another thread misses its check-in.
            supervision_lost = true;
            failed = true;
            set_health(board::HEALTH_FAILED, true, checks);
            let _ = ipc::LED_COMMANDS.try_send(LedCommand::HealthFailed);
        }

        let elapsed = now.saturating_sub(start);
        if pending && !check_done && elapsed >= HEALTH_PASS_MS {
            if conditions_met(&handover, &counts) {
                check_done = true;
                if boot::write_confirmed() {
                    boot::mark_confirmed();
                    set_health(board::HEALTH_PASSED, false, checks);
                    log_info!("health: passed, CONFIRMED written");
                } else {
                    log_error!("health: CONFIRMED record write failed");
                    failed = true;
                    set_health(board::HEALTH_FAILED, true, checks);
                    let _ = ipc::LED_COMMANDS.try_send(LedCommand::HealthFailed);
                }
            } else if elapsed >= HEALTH_LIMIT_MS {
                // HLT-002: the check must have passed by 10 s.
                log_error!("health: conditions not met, trial fails");
                check_done = true;
                failed = true;
                set_health(board::HEALTH_FAILED, true, checks);
                let _ = ipc::LED_COMMANDS.try_send(LedCommand::HealthFailed);
            }
        } else if !pending && !failed {
            set_health(board::HEALTH_NOT_REQUIRED, false, checks);
        }

        // HLT-003: a failed pending slot stops feeding after 10 s so BOOT
        // counts a failed trial.
        let feed = !supervision_lost && !(failed && pending && elapsed >= HEALTH_LIMIT_MS);
        if feed {
            plat::wdt_feed();
        }

        plat::sleep_ms(LOOP_MS);
    }
}
