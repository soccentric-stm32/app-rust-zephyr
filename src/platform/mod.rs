// platform - Rust side of the C platform layer
//
// plat.rs is the FFI surface (platform/platform.c).  board.rs, boot.rs and
// build_info.rs hold the board, bootloader-contract and build metadata that
// the application shares with the C side.

pub mod board;
pub mod boot;
pub mod build_info;
pub mod plat;
