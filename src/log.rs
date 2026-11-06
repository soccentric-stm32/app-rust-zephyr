// log.rs - debug lines on USART6 (RZDBG-001)
//
// Author: Sandesh Ghimire <sandesh@soccentric.com>
// SRS:    RZDBG-001..RZDBG-003
//
// USART3 carries HWLINK only; debug text goes to USART6 through Zephyr's
// deferred logging (src/debug_log.c), in the line format BOOT and the other
// firmwares use:
//
//     [   5012] R-ZEP I health: passed, CONFIRMED written
//
// The macros format into a fixed stack buffer (no allocation) and never
// block.  Text past PLAT_LOG_TEXT_MAX bytes is cut off.

use core::fmt::{self, Write};

use crate::plat;

/// Same limit as PLAT_LOG_TEXT_MAX in src/platform.h.
const TEXT_MAX: usize = 120;

struct Line {
    buf: [u8; TEXT_MAX],
    len: usize,
}

impl Write for Line {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let take = s.len().min(TEXT_MAX - self.len);
        self.buf[self.len..self.len + take].copy_from_slice(&s.as_bytes()[..take]);
        self.len += take;
        Ok(())
    }
}

pub fn write(level: u8, args: fmt::Arguments) {
    let mut line = Line {
        buf: [0; TEXT_MAX],
        len: 0,
    };
    let _ = line.write_fmt(args);
    plat::log(level, &line.buf[..line.len]);
}

macro_rules! log_error {
    ($($arg:tt)*) => { $crate::log::write(b'E', format_args!($($arg)*)) };
}

macro_rules! log_warn {
    ($($arg:tt)*) => { $crate::log::write(b'W', format_args!($($arg)*)) };
}

macro_rules! log_info {
    ($($arg:tt)*) => { $crate::log::write(b'I', format_args!($($arg)*)) };
}
