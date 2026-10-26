// Real Linux descriptor ownership across a whole package call/return.
extern "C" fn dup(fd: i32) -> i32;
extern "C" fn close(fd: i32) -> i32;
extern "C" fn putchar(code: i32) -> i32;
resource Handle { fd: i32 }
impl Drop for Handle { fn drop(self: Self) { unsafe { close(.fd) } } }
package Pair { manages first: Handle; manages second: Handle }
fn duplicate(fd: i32) -> i32 { unsafe { return dup(fd) } }
fn relay(pair: Pair) -> Pair { return pair }
fn main() -> i32 {
    let first = duplicate(0)
    if first < 0 { return 10 }
    let second = duplicate(0)
    if second < 0 { unsafe { close(first) }; return 11 }
    if true {
        let pair = Pair { second: Handle { fd: second }, first: Handle { fd: first } }
        let moved = relay(pair)
        let a = moved.first
        let b = moved.second
        if a.fd != first || b.fd != second { return 12 }
    }
    let a = duplicate(first)
    if a >= 0 { unsafe { close(a) }; return 13 }
    let b = duplicate(second)
    if b >= 0 { unsafe { close(b) }; return 14 }
    unsafe { putchar(70) }
    return 0
}
