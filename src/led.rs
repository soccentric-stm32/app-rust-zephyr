// led.rs - LED thread and Appendix A patterns
//
// Author: Sandesh Ghimire <sandesh@soccentric.com>
// SRS:    RZFN-001; LED-001..LED-007, LED-005 (LD3 reserved)
//
// The pattern logic is the Rust Embassy demo's; the thread drives LD1..LD3
// through the Zephyr GPIO driver and wakes every millisecond.

use crate::hwlink::{led, pattern};
use crate::ipc::{self, ButtonEvent, LedCommand};
use crate::plat;

const DEFAULT_PERIOD_MS: u16 = 1000;
const TICK_MS: u32 = 1;
const CHECKIN_PERIOD_MS: u64 = 100;

const LD1: u32 = 0;
const LD2: u32 = 1;
const LD3: u32 = 2;

fn morse_sos_on(elapsed_ms: u64) -> bool {
    // ··· — — — ··· with a 100 ms unit, including the 3/7 unit gaps.
    matches!(
        elapsed_ms % 3400,
        0..100
            | 200..300
            | 400..500
            | 800..1100
            | 1200..1500
            | 1600..1900
            | 2200..2300
            | 2400..2500
            | 2600..2700
    )
}

fn pattern_on(pattern: u8, period_ms: u16, elapsed_ms: u64, led2: bool) -> bool {
    let period = period_ms.max(50) as u64;
    match pattern {
        pattern::OFF => false,
        pattern::ON => true,
        pattern::BLINK => elapsed_ms % period < period / 2,
        pattern::HEARTBEAT => {
            let cycle = period.max(250);
            let phase = elapsed_ms % cycle;
            phase < 50 || (200..250).contains(&phase)
        }
        pattern::FAST => {
            let fast = (period / 5).max(2);
            elapsed_ms % fast < fast / 2
        }
        pattern::CHASE => {
            let half = period / 2;
            if led2 {
                elapsed_ms % period >= half
            } else {
                elapsed_ms % period < half
            }
        }
        pattern::MORSE_SOS => morse_sos_on(elapsed_ms),
        _ => false,
    }
}

fn update_status(led1: u8, p1: u16, led2: u8, p2: u16, led3: u8, p3: u16, fault: bool) {
    ipc::update_status(|v| {
        v.led1_pattern = led1;
        v.led1_period = p1;
        v.led2_pattern = led2;
        v.led2_period = p2;
        v.led3_pattern = led3;
        v.led3_period = p3;
        v.fault_active = fault;
    });
}

#[zephyr::thread(stack_size = crate::LED_STACK)]
pub fn thread(initial_fault: bool) {
    plat::thread_name(c"LED");

    let start = plat::now_ms();
    let mut pattern1 = pattern::HEARTBEAT;
    let mut pattern2 = pattern::OFF;
    let mut period1 = DEFAULT_PERIOD_MS;
    let mut period2 = DEFAULT_PERIOD_MS;
    let mut phase1 = start;
    let mut phase2 = start;
    let mut fault_active = initial_fault;
    let mut health_failed = false;
    let mut led3_forced: Option<(u8, u16)> = None;
    let mut led3_pattern = pattern::OFF;
    let mut led3_period = DEFAULT_PERIOD_MS;
    let mut led3_phase = start;
    let mut last_checkin = start;

    plat::led_set(LD1, false);
    plat::led_set(LD2, false);
    plat::led_set(LD3, false);

    update_status(
        pattern1,
        period1,
        pattern2,
        period2,
        led3_pattern,
        led3_period,
        fault_active,
    );

    loop {
        while let Some(command) = ipc::LED_COMMANDS.try_receive() {
            match command {
                LedCommand::Set {
                    led,
                    pattern: p,
                    period_ms,
                    force,
                } => match led {
                    crate::hwlink::led::LD1 => {
                        let now = plat::now_ms();
                        if p == pattern::CHASE {
                            pattern1 = p;
                            pattern2 = p;
                            period1 = period_ms;
                            period2 = period_ms;
                            phase1 = now;
                            phase2 = now;
                        } else {
                            if pattern1 == pattern::CHASE || pattern2 == pattern::CHASE {
                                pattern2 = pattern::OFF;
                                period2 = period_ms;
                                phase2 = now;
                            }
                            pattern1 = p;
                            period1 = period_ms;
                            phase1 = now;
                        }
                    }
                    crate::hwlink::led::LD2 => {
                        let now = plat::now_ms();
                        if p == pattern::CHASE {
                            pattern1 = p;
                            pattern2 = p;
                            period1 = period_ms;
                            period2 = period_ms;
                            phase1 = now;
                            phase2 = now;
                        } else {
                            if pattern1 == pattern::CHASE || pattern2 == pattern::CHASE {
                                pattern1 = pattern::OFF;
                                period1 = period_ms;
                                phase1 = now;
                            }
                            pattern2 = p;
                            period2 = period_ms;
                            phase2 = now;
                        }
                    }
                    crate::hwlink::led::LD3 if force => {
                        led3_forced = Some((p, period_ms));
                        led3_pattern = p;
                        led3_period = period_ms;
                        led3_phase = plat::now_ms();
                    }
                    _ => {}
                },
                LedCommand::SetAll {
                    pattern: p,
                    period_ms,
                } => {
                    pattern1 = p;
                    pattern2 = p;
                    period1 = period_ms;
                    period2 = period_ms;
                    let now = plat::now_ms();
                    phase1 = now;
                    phase2 = now;
                }
                LedCommand::ButtonShort => {
                    // BTN-002: the next Appendix A pattern on LD1 and LD2.
                    let now = plat::now_ms();
                    let period = if (50..=5000).contains(&period1) && period1.is_multiple_of(10) {
                        period1
                    } else {
                        DEFAULT_PERIOD_MS
                    };
                    let next = next_pattern(pattern1);
                    pattern1 = next;
                    pattern2 = next;
                    period1 = period;
                    period2 = period;
                    phase1 = now;
                    phase2 = now;
                    let _ = ipc::BUTTON_EVENTS.try_send(ButtonEvent {
                        press: ipc::BUTTON_SHORT,
                        pattern: next,
                        period_ms: period,
                    });
                }
                LedCommand::ButtonLong => {
                    // BTN-003: restore the defaults and report the event.
                    let now = plat::now_ms();
                    pattern1 = pattern::HEARTBEAT;
                    pattern2 = pattern::OFF;
                    period1 = DEFAULT_PERIOD_MS;
                    period2 = DEFAULT_PERIOD_MS;
                    phase1 = now;
                    phase2 = now;
                    let _ = ipc::BUTTON_EVENTS.try_send(ButtonEvent {
                        press: ipc::BUTTON_LONG,
                        pattern: pattern::HEARTBEAT,
                        period_ms: DEFAULT_PERIOD_MS,
                    });
                }
                LedCommand::ClearFault => {
                    // LED-005: health_failed is separate; a pending slot
                    // that failed its health check keeps blinking LD3.
                    fault_active = false;
                }
                LedCommand::HealthFailed => {
                    health_failed = true;
                }
            }
            update_status(
                pattern1,
                period1,
                pattern2,
                period2,
                led3_pattern,
                led3_period,
                fault_active,
            );
        }

        let now = plat::now_ms();
        let elapsed1 = now.saturating_sub(phase1);
        let elapsed2 = now.saturating_sub(phase2);

        plat::led_set(LD1, pattern_on(pattern1, period1, elapsed1, false));
        plat::led_set(LD2, pattern_on(pattern2, period2, elapsed2, true));

        let led3_on = if health_failed {
            // LED-005: fast blink while the slot is pending and health failed.
            now.saturating_sub(led3_phase) % 100 < 50
        } else if fault_active {
            true
        } else if let Some((p, period)) = led3_forced {
            pattern_on(p, period, now.saturating_sub(led3_phase), false)
        } else {
            false
        };
        plat::led_set(LD3, led3_on);

        if now.saturating_sub(last_checkin) >= CHECKIN_PERIOD_MS {
            ipc::check_in(ipc::TASK_LED);
            last_checkin = now;
        }
        plat::sleep_ms(TICK_MS);
    }
}

/// Update a `Status` mirror for a `SetLed` command.  The comms task uses this
/// so that a `GetStatus` immediately after the `Ack` already reports the new
/// configuration; the LED task applies the same rules to its own state.
pub fn mirror_set(snapshot: &mut ipc::StatusSnapshot, target: u8, p: u8, period_ms: u16) {
    match target {
        led::LD1 => {
            if p == pattern::CHASE {
                snapshot.led1_pattern = p;
                snapshot.led1_period = period_ms;
                snapshot.led2_pattern = p;
                snapshot.led2_period = period_ms;
            } else {
                if snapshot.led1_pattern == pattern::CHASE
                    || snapshot.led2_pattern == pattern::CHASE
                {
                    snapshot.led2_pattern = pattern::OFF;
                    snapshot.led2_period = period_ms;
                }
                snapshot.led1_pattern = p;
                snapshot.led1_period = period_ms;
            }
        }
        led::LD2 => {
            if p == pattern::CHASE {
                snapshot.led1_pattern = p;
                snapshot.led1_period = period_ms;
                snapshot.led2_pattern = p;
                snapshot.led2_period = period_ms;
            } else {
                if snapshot.led1_pattern == pattern::CHASE
                    || snapshot.led2_pattern == pattern::CHASE
                {
                    snapshot.led1_pattern = pattern::OFF;
                    snapshot.led1_period = period_ms;
                }
                snapshot.led2_pattern = p;
                snapshot.led2_period = period_ms;
            }
        }
        led::LD3 => {
            snapshot.led3_pattern = p;
            snapshot.led3_period = period_ms;
        }
        _ => {}
    }
}

pub fn mirror_set_all(snapshot: &mut ipc::StatusSnapshot, p: u8, period_ms: u16) {
    snapshot.led1_pattern = p;
    snapshot.led1_period = period_ms;
    snapshot.led2_pattern = p;
    snapshot.led2_period = period_ms;
}

pub fn next_pattern(current: u8) -> u8 {
    // Appendix A order, skipping the reserved error LED.
    match current {
        pattern::OFF => pattern::ON,
        pattern::ON => pattern::BLINK,
        pattern::BLINK => pattern::HEARTBEAT,
        pattern::HEARTBEAT => pattern::FAST,
        pattern::FAST => pattern::CHASE,
        pattern::CHASE => pattern::MORSE_SOS,
        _ => pattern::OFF,
    }
}
