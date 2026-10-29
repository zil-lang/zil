pub fn append<F: FnMut(&mut [u8], &mut i32) -> i32>(callback: &mut F, window: &mut [u8], counter: &mut i32) -> i32 {
    return callback(window, counter)
}
pub fn repeat<F: FnMut(&mut [u8], &mut i32) -> i32>(callback: &mut F, window: &mut [u8], counter: &mut i32, count: i64) -> i32 {
    for index in 0..count {
        let length = append(callback, window, counter)
        if length < 0 { return 1 }
        if i64(length) != index + 1 { return 7 }
        if i32(window[0]) != *counter - 1 { return 8 }
        emit(i32(window[0]))
    }
    return 0
}
extern "C" fn putchar(value: i32) -> i32;
pub fn emit(value: i32) { unsafe { putchar(value) } }
