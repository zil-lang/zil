// Observe real close(2) effects through dup(2), including a replaced field.
extern "C" fn dup(fd: i32) -> i32;
extern "C" fn close(fd: i32) -> i32;
extern "C" fn putchar(code: i32) -> i32;
resource Handle { fd: i32 }
impl Drop for Handle { fn drop(self: Self) { unsafe { close(.fd) } } }
package Pair { manages first: Handle; manages second: Handle }
fn duplicate(fd: i32) -> i32 { unsafe { return dup(fd) } }
fn closed(fd: i32) -> bool {
    let probe = duplicate(fd)
    if probe >= 0 { unsafe { close(probe) }; return false }
    return true
}
fn main() -> i32 {
    let first = duplicate(0)
    if first < 0 { return 10 }
    let second = duplicate(0)
    if second < 0 { unsafe { close(first) }; return 11 }
    let third = duplicate(0)
    if third < 0 { unsafe { close(first); close(second) }; return 12 }
    if true {
        var pair = Pair { first: Handle { fd: first }, second: Handle { fd: second } }
        pair.first = Handle { fd: third }
        if !closed(first) { return 13 }
        // The moved field is independently cleaned at the nested scope exit.
        if true { let removed = give pair.second }
        if !closed(second) { return 14 }
        if pair.first.fd != third { return 15 }
    }
    if !closed(third) { return 16 }
    unsafe { putchar(70) }
    return 0
}
