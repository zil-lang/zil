//! Independent native descriptor oracle; unsafe is confined to libc calls.
use std::io::{self, Write};
unsafe extern "C" {
    fn dup(fd: i32) -> i32;
    fn close(fd: i32) -> i32;
}
struct Handle(i32);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            close(self.0);
        }
    }
}
struct Pair {
    first: Handle,
    second: Handle,
}
fn relay(pair: Pair) -> Pair {
    pair
}
fn application() -> i32 {
    let first = unsafe { dup(0) };
    if first < 0 {
        return 10;
    }
    let second = unsafe { dup(0) };
    if second < 0 {
        unsafe {
            close(first);
        }
        return 11;
    }
    {
        let pair = Pair {
            second: Handle(second),
            first: Handle(first),
        };
        let moved = relay(pair);
        if moved.first.0 != first || moved.second.0 != second {
            return 12;
        }
    }
    let a = unsafe { dup(first) };
    if a >= 0 {
        unsafe {
            close(a);
        }
        return 13;
    }
    let b = unsafe { dup(second) };
    if b >= 0 {
        unsafe {
            close(b);
        }
        return 14;
    }
    io::stdout().write_all(b"F").unwrap();
    0
}
fn main() {
    std::process::exit(application());
}
