//! Same fixed initialized-buffer and synchronous C contract as io.zil.
#![allow(dead_code)]
unsafe extern "C" {
    #[link_name = "read"]
    fn c_read(fd: i32, buffer: *mut core::ffi::c_void, count: usize) -> isize;
    #[link_name = "write"]
    fn c_write(fd: i32, buffer: *const core::ffi::c_void, count: usize) -> isize;
    #[link_name = "pipe"]
    fn c_pipe(fds: *mut i32) -> i32;
    #[link_name = "close"]
    fn c_close(fd: i32) -> i32;
    #[link_name = "dup"]
    fn c_dup(fd: i32) -> i32;
    #[link_name = "putchar"]
    fn c_putchar(code: i32) -> i32;
}
pub fn empty() -> [u8; 64] {
    [0; 64]
}
pub fn read_chunk(fd: i32, buffer: &mut [u8; 64]) -> i32 {
    unsafe { i32::try_from(c_read(fd, buffer.as_mut_ptr().cast(), 64)).unwrap() }
}
// A successful partial write consumes its prefix; the remainder is moved to
// the beginning of the same array, exactly as in the ZIL implementation.
pub fn write_all(fd: i32, buffer: &mut [u8; 64], count: i32) -> i32 {
    if !(0..=64).contains(&count) {
        return -2;
    }
    let mut remaining = count;
    while remaining > 0 {
        let written = unsafe { c_write(fd, buffer.as_ptr().cast(), remaining as usize) };
        if written <= 0 || written > remaining as isize {
            return -1;
        }
        let used = i32::try_from(written).unwrap();
        remaining -= used;
        let mut index = 0;
        while index < remaining {
            buffer[index as usize] = buffer[(index + used) as usize];
            index += 1;
        }
    }
    0
}
pub fn open_pipe() -> [i32; 2] {
    let mut fds = [-1, -1];
    if unsafe { c_pipe(fds.as_mut_ptr()) } != 0 {
        return [-1, -1];
    }
    fds
}
pub fn close_fd(fd: i32) {
    unsafe {
        c_close(fd);
    }
}
pub fn closed(fd: i32) -> bool {
    let probe = unsafe { c_dup(fd) };
    if probe >= 0 {
        close_fd(probe);
        false
    } else {
        true
    }
}
pub fn emit(value: i32) {
    unsafe {
        c_putchar(value);
    }
}
fn digits(value: i64) {
    if value >= 10 {
        digits(value / 10);
    }
    emit(48 + i32::try_from(value % 10).unwrap());
}
pub fn number(value: i64) {
    digits(value);
    emit(10);
}
