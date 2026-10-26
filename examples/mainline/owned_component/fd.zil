// Observe real close effects after factory return, replacement and env drop.
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
fn keeper(pair: Pair) -> impl FnMut(i32) -> i32 {
    var owned = pair
    return move (replacement) => {
        if replacement >= 0 { owned.first = Handle { fd: replacement } }
        return owned.second.fd
    }
}
fn main() -> i32 {
    let first = duplicate(0)
    if first < 0 { return 10 }
    let second = duplicate(0)
    if second < 0 { unsafe { close(first) }; return 11 }
    let third = duplicate(0)
    if third < 0 { unsafe { close(first); close(second) }; return 12 }
    if true {
        var kept = keeper(Pair { first: Handle { fd: first }, second: Handle { fd: second } })
        if kept(-1) != second || closed(first) || closed(second) { return 13 }
        if kept(third) != second { return 14 }
        if !closed(first) || closed(second) || closed(third) { return 15 }
        if kept(-1) != second { return 16 }
    }
    if !closed(second) || !closed(third) { return 17 }
    unsafe { putchar(70) }
    return 0
}
