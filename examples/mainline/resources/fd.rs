//! Independent Rust OwnedFd oracle for the Linux native resource example.
use std::{
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    process::ExitCode,
};
unsafe extern "C" {
    fn dup(fd: i32) -> i32;
    fn putchar(value: i32) -> i32;
}
fn handoff(handle: OwnedFd) -> OwnedFd {
    handle
}
fn run() -> u8 {
    let raw = unsafe { dup(0) };
    if raw < 0 {
        return 10;
    }
    {
        // A successful dup returns a new descriptor owned by this function.
        let first = unsafe { OwnedFd::from_raw_fd(raw) };
        let alias = &first;
        if alias.as_raw_fd() != raw {
            return 11;
        }
        let second = handoff(first);
        if second.as_raw_fd() != raw {
            return 12;
        }
    }
    let probe = unsafe { dup(raw) };
    if probe >= 0 {
        drop(unsafe { OwnedFd::from_raw_fd(probe) });
        return 13;
    }
    unsafe {
        putchar(70);
    }
    0
}
fn main() -> ExitCode {
    ExitCode::from(run())
}
