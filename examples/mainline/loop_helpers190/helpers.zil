pub fn append<F: FnMut(&mut i32) -> i32>(callback: &mut F, value: &mut i32) -> i32 {
    return callback(value)
}

pub fn repeat<F: FnMut(&mut i32) -> i32>(callback: &mut F, value: &mut i32, observed: ByteBuffer, count: i64) -> i32 {
    var index: i64 = 0
    while index < count {
        let length = append(callback, value)
        if length < 0 { return 1 }
        index += 1
        if i64(length) != index + 1 { return 7 }
        if !bytes_reserve(observed, index) { return 1 }
        if !bytes_push(observed, u8(*value - 1)) { return 2 }
        let view = bytes_view(observed)
        emit(i32(view[index - 1]))
    }
    return 0
}

extern "C" fn putchar(value: i32) -> i32;
pub fn emit(value: i32) { unsafe { putchar(value) } }
