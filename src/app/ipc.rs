// ipc.rs - shared state and queues between the four Zephyr threads
//
// Author: Sandesh Ghimire <sandesh@soccentric.com>
// SRS:    RZRT-002..005
//
// Everything is static and fixed-size; there is no allocator.  The
// critical sections come from the zephyr crate (irq_lock()), and are only
// held to copy a few bytes.

use core::cell::{Cell, RefCell};
use core::sync::atomic::{AtomicU32, Ordering};

use critical_section::Mutex;

pub const TASK_LED: usize = 0;
pub const TASK_BUTTON: usize = 1;
pub const TASK_COMMS: usize = 2;
pub const TASK_HEALTH: usize = 3;
pub const TASK_COUNT: usize = 4;

#[derive(Clone, Copy)]
pub struct StatusSnapshot {
    pub led1_pattern: u8,
    pub led1_period: u16,
    pub led2_pattern: u8,
    pub led2_period: u16,
    pub led3_pattern: u8,
    pub led3_period: u16,
    pub health_state: u8,
    pub fault_active: bool,
    pub health_failed: bool,
    pub task_checks: u32,
}

impl StatusSnapshot {
    pub const fn initial() -> Self {
        Self {
            led1_pattern: crate::protocol::hwlink::pattern::HEARTBEAT,
            led1_period: 1000,
            led2_pattern: crate::protocol::hwlink::pattern::OFF,
            led2_period: 1000,
            led3_pattern: crate::protocol::hwlink::pattern::OFF,
            led3_period: 1000,
            health_state: crate::platform::board::HEALTH_NOT_REQUIRED,
            fault_active: false,
            health_failed: false,
            task_checks: 0,
        }
    }
}

static STATUS: Mutex<Cell<StatusSnapshot>> = Mutex::new(Cell::new(StatusSnapshot::initial()));

pub fn status() -> StatusSnapshot {
    critical_section::with(|cs| STATUS.borrow(cs).get())
}

pub fn update_status(f: impl FnOnce(&mut StatusSnapshot)) {
    critical_section::with(|cs| {
        let cell = STATUS.borrow(cs);
        let mut snapshot = cell.get();
        f(&mut snapshot);
        cell.set(snapshot);
    });
}

/// A small fixed-capacity FIFO for `Copy` messages.
pub struct Queue<T: Copy, const N: usize> {
    inner: Mutex<RefCell<Ring<T, N>>>,
}

struct Ring<T: Copy, const N: usize> {
    items: [Option<T>; N],
    head: usize,
    len: usize,
}

impl<T: Copy, const N: usize> Queue<T, N> {
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(RefCell::new(Ring {
                items: [None; N],
                head: 0,
                len: 0,
            })),
        }
    }

    /// Queue a message; `Err` when the queue is full.
    pub fn try_send(&self, item: T) -> Result<(), T> {
        critical_section::with(|cs| {
            let mut ring = self.inner.borrow_ref_mut(cs);
            if ring.len == N {
                return Err(item);
            }
            let tail = (ring.head + ring.len) % N;
            ring.items[tail] = Some(item);
            ring.len += 1;
            Ok(())
        })
    }

    pub fn try_receive(&self) -> Option<T> {
        critical_section::with(|cs| {
            let mut ring = self.inner.borrow_ref_mut(cs);
            if ring.len == 0 {
                return None;
            }
            let head = ring.head;
            let item = ring.items[head].take();
            ring.head = (head + 1) % N;
            ring.len -= 1;
            item
        })
    }
}

#[derive(Clone, Copy)]
pub enum LedCommand {
    Set {
        led: u8,
        pattern: u8,
        period_ms: u16,
        force: bool,
    },
    SetAll {
        pattern: u8,
        period_ms: u16,
    },
    ButtonShort,
    ButtonLong,
    ClearFault,
    HealthFailed,
}

pub static LED_COMMANDS: Queue<LedCommand, 8> = Queue::new();

pub const BUTTON_SHORT: u8 = 1;
pub const BUTTON_LONG: u8 = 2;

#[derive(Clone, Copy)]
pub struct ButtonEvent {
    pub press: u8,
    pub pattern: u8,
    pub period_ms: u16,
}

pub static BUTTON_EVENTS: Queue<ButtonEvent, 4> = Queue::new();

// One counter per thread, so several check-ins between two health polls are
// all counted (the C demo's k_event would merge them).
static CHECKINS: [AtomicU32; TASK_COUNT] = [
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
];

// RZRT-005: smallest unused stack seen per thread, in bytes.
static STACK_UNUSED: [AtomicU32; TASK_COUNT] = [
    AtomicU32::new(u32::MAX),
    AtomicU32::new(u32::MAX),
    AtomicU32::new(u32::MAX),
    AtomicU32::new(u32::MAX),
];

/// Called by each thread at least every 500 ms (RZRT-004).  It also samples
/// the calling thread's stack high-water mark for `Status`; the stack scan
/// is why threads with a fast loop check in on their own slower cadence.
pub fn check_in(task: usize) {
    CHECKINS[task].fetch_add(1, Ordering::Relaxed);
    STACK_UNUSED[task].fetch_min(crate::platform::plat::stack_unused(), Ordering::Relaxed);
}

/// Return and reset the number of check-ins since the last call.
pub fn take_check_in(task: usize) -> u32 {
    CHECKINS[task].swap(0, Ordering::Relaxed)
}

pub fn stack_unused(task: usize) -> u32 {
    match STACK_UNUSED[task].load(Ordering::Relaxed) {
        u32::MAX => 0,
        unused => unused,
    }
}
