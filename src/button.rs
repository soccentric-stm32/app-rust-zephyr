// button.rs - B1 (PC13) debounce, short/long press
//
// Author: Sandesh Ghimire <sandesh@soccentric.com>
// SRS:    RZFN-002; BTN-001..BTN-004
//
// The thread only debounces the button and sends one command per press.  The
// LED thread owns the pattern state (as in the C demo), so a second press that
// arrives before the first command has been handled can never lose a step.
// The GPIO interrupt wakes the thread on an edge; the 10 ms timeout keeps the
// debounce and the health check-in running while the button is held.

use crate::ipc::{self, LedCommand};
use crate::plat;

const DEBOUNCE_MS: u64 = 30;
const SHORT_MAX_MS: u64 = 1000;
const LONG_MIN_MS: u64 = 2000;
const POLL_MS: u32 = 10;
const CHECKIN_PERIOD_MS: u64 = 500;

#[zephyr::thread(stack_size = crate::BUTTON_STACK)]
pub fn thread() {
    plat::thread_name(c"Button");

    // B1 is high when pressed and has an external pull-down on the board.
    let mut pressed = plat::button_pressed();
    let mut candidate = pressed;
    let mut candidate_since = plat::now_ms();
    let mut press_start = candidate_since;
    let mut long_sent = false;
    let mut last_checkin = candidate_since;

    let mut report = crate::log::TaskReport::new(
        2,
        "Button",
        "debounces B1, short press cycles the LED pattern, long press resets it",
    );

    loop {
        report.tick();
        plat::button_wait(POLL_MS);

        let now = plat::now_ms();
        let raw = plat::button_pressed();
        if raw != candidate {
            candidate = raw;
            candidate_since = now;
        } else if candidate != pressed && now - candidate_since >= DEBOUNCE_MS {
            pressed = candidate;
            if pressed {
                press_start = now;
                long_sent = false;
            } else if !long_sent && now - press_start < SHORT_MAX_MS {
                log_info!("button: short press, next LED pattern");
                let _ = ipc::LED_COMMANDS.try_send(LedCommand::ButtonShort);
            }
        }

        // BTN-003: a long press acts at 2 s, not only when the button is
        // released, exactly as the C demo's button thread does.
        if pressed && !long_sent && now - press_start >= LONG_MIN_MS {
            long_sent = true;
            log_info!("button: long press, default LED patterns");
            let _ = ipc::LED_COMMANDS.try_send(LedCommand::ButtonLong);
        }

        if now - last_checkin >= CHECKIN_PERIOD_MS {
            ipc::check_in(ipc::TASK_BUTTON);
            last_checkin = now;
        }
    }
}
