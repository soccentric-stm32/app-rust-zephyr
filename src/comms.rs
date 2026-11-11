// comms.rs - USART3 HWLINK thread and demo message handling
//
// Author: Sandesh Ghimire <sandesh@soccentric.com>
// SRS:    RZFN-003, RZFN-005; RZLNK-001..004; COM-001..COM-007
//
// The message handling is the Rust Embassy demo's.  The thread blocks on the
// UART receive semaphore (platform.c) for at most 20 ms, so button events
// and the 921600 baud fall-back are served without a second thread.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::boot::{self, Handover};
use crate::build_info;
use crate::fault;
use crate::hwlink::frame::{self, FLAG_EVENT, FLAG_RESPONSE, MAX_DECODED, MAX_ENCODED};
use crate::hwlink::{self, error, led, pattern, ParseError, Request};
use crate::identity;
use crate::ipc::{self, ButtonEvent, LedCommand};
use crate::plat;

const FIRMWARE_NAME: &str = "rust-zephyr-app";
const PAYLOAD_MAX: usize = 1024;
const BAUD_DEFAULT: u32 = 115_200;
const BAUD_FAST: u32 = 921_600;
const BAUD_REVERT_MS: u64 = 5000;
const WAIT_MS: u32 = 20;
const CHECKIN_PERIOD_MS: u64 = 500;
const TX_TIMEOUT_MS: u32 = 200;

#[derive(Clone, Copy)]
pub struct BootInfo {
    pub reset_cause: u32,
    pub handover: Handover,
}

#[derive(Clone, Copy)]
enum Action {
    None,
    Hello,
    Reset,
    EnterBootloader,
    SetBaud(u32),
}

/// Frame buffers, about 9 KB: kept out of the thread stack.
struct Buffers {
    decoded: [u8; MAX_DECODED],
    raw: [u8; MAX_DECODED],
    tx: [u8; MAX_ENCODED],
    payload: [u8; PAYLOAD_MAX],
    last: [u8; MAX_ENCODED],
}

/// Hands out `&'static mut Buffers` exactly once, to the comms thread.
struct BufferCell {
    taken: AtomicBool,
    buffers: UnsafeCell<Buffers>,
}

// SAFETY: the buffers are reachable only through take(), which succeeds once.
unsafe impl Sync for BufferCell {}

impl BufferCell {
    // The same one-shot hand-out as the static_cell crate: the flag makes the
    // &mut unique, which clippy cannot see from the signature.
    #[allow(clippy::mut_from_ref)]
    fn take(&'static self) -> Option<&'static mut Buffers> {
        if self.taken.swap(true, Ordering::AcqRel) {
            None
        } else {
            // SAFETY: first and only caller, see above.
            Some(unsafe { &mut *self.buffers.get() })
        }
    }
}

static BUFFERS: BufferCell = BufferCell {
    taken: AtomicBool::new(false),
    buffers: UnsafeCell::new(Buffers {
        decoded: [0; MAX_DECODED],
        raw: [0; MAX_DECODED],
        tx: [0; MAX_ENCODED],
        payload: [0; PAYLOAD_MAX],
        last: [0; MAX_ENCODED],
    }),
};

struct Link {
    decoder: frame::Decoder,
    last_seq: u16,
    last_len: usize,
    revert_at: Option<u64>,
    open: bool,
}

fn valid_led_period(period_ms: u16) -> bool {
    (50..=5000).contains(&period_ms) && period_ms.is_multiple_of(10)
}

fn touch_baud(link: &mut Link) {
    if link.revert_at.is_some() {
        link.revert_at = Some(plat::now_ms() + BAUD_REVERT_MS);
    }
}

fn build_nak(out: &mut [u8], seq: u16, code: u16, detail: &str) -> usize {
    hwlink::nak(out, seq, code, detail)
        .map(|p| p.len())
        .unwrap_or(0)
}

fn status_reply(out: &mut [u8], info: &BootInfo) -> Option<usize> {
    let snap = ipc::status();
    let fault_record = fault::last();
    let active_fault = fault_record.as_ref().map(|f| hwlink::FaultInfoData {
        present: true,
        reason: fault::describe(f),
        cfsr: f.cfsr,
        hfsr: f.hfsr,
        mmfar: f.mmfar,
        bfar: f.bfar,
        pc: f.pc,
        lr: f.lr,
        task: fault::text(&f.task),
        file: fault::text(&f.file),
        line: f.line,
    });
    let leds = [
        hwlink::LedStateData {
            pattern: snap.led1_pattern,
            period_ms: snap.led1_period,
        },
        hwlink::LedStateData {
            pattern: snap.led2_pattern,
            period_ms: snap.led2_period,
        },
        hwlink::LedStateData {
            pattern: snap.led3_pattern,
            period_ms: snap.led3_period,
        },
    ];
    let stack = |task| ipc::stack_unused(task).min(u16::MAX as u32) as u16;
    let tasks = [
        hwlink::TaskStatsData {
            name: "Health",
            priority: crate::PRIO_HEALTH as u8,
            state: 1,
            stack_high_water: stack(ipc::TASK_HEALTH),
        },
        hwlink::TaskStatsData {
            name: "Comms",
            priority: crate::PRIO_COMMS as u8,
            state: 1,
            stack_high_water: stack(ipc::TASK_COMMS),
        },
        hwlink::TaskStatsData {
            name: "Button",
            priority: crate::PRIO_BUTTON as u8,
            state: 1,
            stack_high_water: stack(ipc::TASK_BUTTON),
        },
        hwlink::TaskStatsData {
            name: "LED",
            priority: crate::PRIO_LED as u8,
            state: 1,
            stack_high_water: stack(ipc::TASK_LED),
        },
    ];
    let status = hwlink::Status {
        firmware_name: FIRMWARE_NAME,
        version: build_info::VERSION,
        commit: build_info::GIT_COMMIT,
        build_time: build_info::BUILD_TIME,
        uid: boot::device_uid(),
        uptime_ms: plat::now_ms(),
        slot: info.handover.slot,
        trial: info.handover.current_trial_state(),
        trial_count: info.handover.trial_count,
        health: snap.health_state,
        reset_cause: info.reset_cause,
        reset_cause_text: boot::reset_cause_text(info.reset_cause),
        fault: active_fault,
        leds,
        tasks: &tasks,
        // The application is heapless (CONFIG_RUST_ALLOC is off).
        heap_free: 0,
        heap_min_free: 0,
    };
    hwlink::status(out, &status).map(|p| p.len())
}

fn reply(request: Request<'_>, seq: u16, out: &mut [u8], info: &BootInfo) -> (Action, usize) {
    let mut action = Action::None;

    let len = match request {
        Request::Hello { .. } => {
            let (dev_id, rev_id) = boot::device_id();
            let mut identity_buf = [0u8; 192];
            let hello = hwlink::HelloReply {
                firmware_name: FIRMWARE_NAME,
                version: build_info::VERSION_TEXT,
                commit: build_info::GIT_COMMIT,
                build_time: build_info::BUILD_TIME,
                uid: boot::device_uid(),
                dev_id,
                rev_id,
                flash_mode: hwlink::flash_mode::DUAL_BANK,
                detail: identity::detail(&mut identity_buf),
            };
            hwlink::hello_reply(out, &hello).map(|p| {
                action = Action::Hello;
                p.len()
            })
        }
        Request::Ping => hwlink::pong(out, plat::now_ms()).map(|p| p.len()),
        Request::GetStatus => status_reply(out, info),
        Request::SetLed {
            led: target,
            pattern: p,
            period_ms,
            force,
        } => {
            if !(led::LD1..=led::LD3).contains(&target)
                || !pattern::valid(p)
                || !valid_led_period(period_ms)
                || (target == led::LD3 && !force)
            {
                Some(build_nak(
                    out,
                    seq,
                    error::BAD_ARGUMENT,
                    "LED, pattern or period",
                ))
            } else {
                let command = LedCommand::Set {
                    led: target,
                    pattern: p,
                    period_ms,
                    force,
                };
                if ipc::LED_COMMANDS.try_send(command).is_err() {
                    Some(build_nak(out, seq, error::BUSY, "LED queue full"))
                } else {
                    ipc::update_status(|v| crate::led::mirror_set(v, target, p, period_ms));
                    hwlink::ack(out, seq).map(|r| r.len())
                }
            }
        }
        Request::SetLedAll {
            pattern: p,
            period_ms,
        } => {
            if !pattern::valid(p) || !valid_led_period(period_ms) {
                Some(build_nak(
                    out,
                    seq,
                    error::BAD_ARGUMENT,
                    "pattern or period",
                ))
            } else {
                let command = LedCommand::SetAll {
                    pattern: p,
                    period_ms,
                };
                if ipc::LED_COMMANDS.try_send(command).is_err() {
                    Some(build_nak(out, seq, error::BUSY, "LED queue full"))
                } else {
                    ipc::update_status(|v| crate::led::mirror_set_all(v, p, period_ms));
                    hwlink::ack(out, seq).map(|r| r.len())
                }
            }
        }
        Request::ClearFault => {
            fault::clear();
            let _ = ipc::LED_COMMANDS.try_send(LedCommand::ClearFault);
            hwlink::ack(out, seq).map(|r| r.len())
        }
        Request::EnterBootloader => hwlink::ack(out, seq).map(|r| {
            action = Action::EnterBootloader;
            r.len()
        }),
        Request::Reset => hwlink::ack(out, seq).map(|r| {
            action = Action::Reset;
            r.len()
        }),
        Request::SetBaud { rate } => {
            if rate == BAUD_DEFAULT || rate == BAUD_FAST {
                hwlink::ack(out, seq).map(|r| {
                    action = Action::SetBaud(rate);
                    r.len()
                })
            } else {
                Some(build_nak(out, seq, error::BAD_ARGUMENT, "baud rate"))
            }
        }
        Request::Unknown => Some(build_nak(out, seq, error::UNSUPPORTED, "message")),
    };

    let len = len.unwrap_or_else(|| build_nak(out, seq, error::MALFORMED, "encode"));
    (action, len)
}

fn handle_payload(payload: &[u8], seq: u16, out: &mut [u8], info: &BootInfo) -> (Action, usize) {
    match hwlink::parse_request(payload) {
        Ok(request) => reply(request, seq, out, info),
        Err(ParseError::Unsupported) => (
            Action::None,
            build_nak(out, seq, error::UNSUPPORTED, "message"),
        ),
        Err(ParseError::Malformed) => (
            Action::None,
            build_nak(out, seq, error::MALFORMED, "verifier"),
        ),
    }
}

fn send_event(b: &mut Buffers, event: ButtonEvent) {
    let Some(p) = hwlink::button_event(&mut b.payload, event.press, event.pattern, event.period_ms)
    else {
        return;
    };
    let len = p.len();
    if let Some(n) = frame::encode(&mut b.raw, &mut b.tx, FLAG_EVENT, 0, &b.payload[..len]) {
        let _ = plat::uart_send(&b.tx[..n], TX_TIMEOUT_MS);
    }
}

fn set_baud(link: &mut Link, rate: u32) {
    link.decoder = frame::Decoder::new();
    if plat::uart_set_baud(rate) {
        link.revert_at = if rate == BAUD_DEFAULT {
            None
        } else {
            Some(plat::now_ms() + BAUD_REVERT_MS)
        };
    } else {
        let _ = plat::uart_set_baud(BAUD_DEFAULT);
        link.revert_at = None;
    }
}

fn handle_frame(b: &mut Buffers, len: usize, link: &mut Link, info: &BootInfo) {
    let Some(f) = frame::parse(&b.decoded[..len]) else {
        return;
    };
    touch_baud(link);
    if f.flags & (FLAG_RESPONSE | FLAG_EVENT) != 0 {
        return;
    }

    // A new host session starts at sequence 1; never answer its Hello from
    // the replay cache.
    if matches!(hwlink::parse_request(f.payload), Ok(Request::Hello { .. })) {
        link.last_len = 0;
    }

    // LNK-008: a repeated seq resends the last response without re-executing.
    if f.seq != 0 && link.last_seq == f.seq && link.last_len != 0 {
        let _ = plat::uart_send(&b.last[..link.last_len], TX_TIMEOUT_MS);
        touch_baud(link);
        return;
    }

    let seq = f.seq;
    let (action, len) = handle_payload(f.payload, seq, &mut b.payload, info);
    if len == 0 {
        return;
    }
    let Some(n) = frame::encode(&mut b.raw, &mut b.tx, FLAG_RESPONSE, seq, &b.payload[..len])
    else {
        return;
    };

    // uart_send() returns once the last stop bit is out, so Reset and
    // EnterBootloader below cannot cut the Ack short.
    if !plat::uart_send(&b.tx[..n], TX_TIMEOUT_MS) {
        return;
    }

    // Cache only after the frame was sent: a failed TX must not make the
    // host's retry replay a response for a request that never ran.
    link.last_seq = seq;
    link.last_len = n;
    b.last[..n].copy_from_slice(&b.tx[..n]);
    touch_baud(link);

    match action {
        Action::None => {}
        Action::Hello => link.open = true,
        Action::Reset => boot::reset(),
        Action::EnterBootloader => boot::enter_bootloader(),
        Action::SetBaud(rate) => set_baud(link, rate),
    }
}

#[zephyr::thread(stack_size = crate::COMMS_STACK)]
pub fn thread(info: BootInfo) {
    plat::thread_name(c"Comms");

    let Some(b) = BUFFERS.take() else {
        fault::fatal();
    };
    let mut link = Link {
        decoder: frame::Decoder::new(),
        last_seq: 0,
        last_len: 0,
        revert_at: None,
        open: false,
    };
    let mut rx = [0u8; 64];
    let mut last_checkin = plat::now_ms();
    ipc::check_in(ipc::TASK_COMMS);

    let mut report = crate::log::TaskReport::new(
        4,
        "Comms",
        "serves HWLINK on USART3: info, identity, button and LED commands",
    );

    loop {
        report.tick();
        plat::uart_wait(WAIT_MS);

        loop {
            let n = plat::uart_read(&mut rx);
            if n == 0 {
                break;
            }
            for &byte in &rx[..n] {
                if link.decoder.push(byte) {
                    if let Some(len) = link.decoder.take(&mut b.decoded) {
                        handle_frame(b, len, &mut link, &info);
                    }
                }
            }
        }

        while let Some(event) = ipc::BUTTON_EVENTS.try_receive() {
            if link.open {
                send_event(b, event);
            }
        }

        let now = plat::now_ms();
        if link.revert_at.is_some_and(|deadline| now >= deadline) {
            // COM-002: no valid frame for 5 s at 921600, back to 115200.
            set_baud(&mut link, BAUD_DEFAULT);
        }

        if now.saturating_sub(last_checkin) >= CHECKIN_PERIOD_MS {
            ipc::check_in(ipc::TASK_COMMS);
            last_checkin = now;
        }
    }
}
