// Linux x86_64: fixed, fully initialized buffers. Foreign calls do not retain
// addresses, reenter ZIL, or unwind; counts are bounded by the 64-byte object.
extern "C" fn read(fd: i32, buffer: &mut [u8; 64], count: i64) -> i64
contract { provides(noescape(buffer)); effects(ffi); };
extern "C" fn write(fd: i32, buffer: &[u8; 64], count: i64) -> i64
contract { provides(noescape(buffer)); effects(ffi); };
extern "C" fn pipe(fds: &mut [i32; 2]) -> i32
contract { provides(noescape(fds)); effects(ffi); };
extern "C" fn close(fd: i32) -> i32;
extern "C" fn dup(fd: i32) -> i32;
extern "C" fn putchar(code: i32) -> i32;

pub fn empty() -> [u8; 64] { return [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] }
pub fn read_chunk(fd: i32, buffer: &mut [u8; 64]) -> i32 {
    unsafe { return i32(read(fd, buffer, 64)) }
}
// This wrapper consumes the buffer contents: a short write shifts only the
// remaining initialized prefix before retrying. No pointer offset is invented.
pub fn write_all(fd: i32, buffer: &mut [u8; 64], count: i32) -> i32 {
    if count < 0 || count > 64 { return -2 }
    var remaining = count
    while remaining > 0 {
        var written: i64 = 0
        unsafe { written = write(fd, &*buffer, i64(remaining)) }
        if written <= 0 || written > i64(remaining) { return -1 }
        let used = i32(written)
        remaining = remaining - used
        var index = 0
        while index < remaining {
            buffer[index] = buffer[index + used]
            index = index + 1
        }
    }
    return 0
}
pub fn open_pipe() -> [i32; 2] {
    var fds: [i32; 2] = [-1, -1]
    unsafe { if pipe(&mut fds) != 0 { return [-1, -1] } }
    return fds
}
pub fn close_fd(fd: i32) { unsafe { close(fd) } }
pub fn closed(fd: i32) -> bool {
    var probe = -1
    unsafe { probe = dup(fd) }
    if probe >= 0 { close_fd(probe); return false }
    return true
}
pub fn emit(value: i32) { unsafe { putchar(value) } }
fn digits(value: i64) {
    if value >= 10 { digits(value / 10) }
    emit(48 + i32(value % 10))
}
pub fn number(value: i64, offset: i64) { digits(value + offset); emit(10) }
