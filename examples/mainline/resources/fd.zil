// Linux native profile. The test supplies an open stdin pipe. Only a successful
// dup result enters the resource, and all raw descriptor calls stay unsafe.
extern "C" fn dup(fd: i32) -> i32;
extern "C" fn close(fd: i32) -> i32;
extern "C" fn putchar(value: i32) -> i32;

resource Handle { fd: i32 }
impl Drop for Handle {
    fn drop(self: Self) { unsafe { close(.fd) } }
}
fn handoff(give handle: Handle) -> Handle contract { result(give) } {
    return give handle
}
fn duplicate(fd: i32) -> i32 { unsafe { return dup(fd) } }

fn main() -> i32 {
    let raw = duplicate(0)
    if raw < 0 { return 10 }
    if true {
        let first = Handle { fd: raw }
        let alias = first
        if alias.fd != raw { return 11 }
        let second = handoff(give first)
        if second.fd != raw { return 12 }
    }
    let probe = duplicate(raw)
    if probe >= 0 {
        unsafe { close(probe) }
        return 13
    }
    unsafe { putchar(70) }
    return 0
}
