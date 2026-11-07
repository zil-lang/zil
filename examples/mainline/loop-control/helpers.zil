extern "C" fn putchar(value: i32) -> i32;
pub fn emit(value: i32) { unsafe { putchar(value) } }

resource Iteration { code: i32 }
impl Drop for Iteration { fn drop(self: Self) { emit(.code) } }

pub fn append<F: FnMut(&mut i32) -> i32>(callback: &mut F, value: &mut i32) -> i32 {
    return callback(value)
}

pub fn repeat<F: FnMut(&mut i32) -> i32>(callback: &mut F, value: &mut i32, observed: ByteBuffer, count: i64) -> i32 {
    var complete: i64 = 0
    for index in 0..count {
        let guard = Iteration { code: 48 + i32(index) }
        let scratch = bytes_new()
        if !bytes_reserve(scratch, 1) { return 1 }
        if !bytes_push(scratch, u8(index)) { return 2 }
        let view = bytes_view(scratch)
        if i64(view[0]) != index { return 8 }
        if index == 1 { continue }
        if index == 4 { break }
        let length = append(callback, value)
        if length < 0 { return 1 }
        if i64(length) != complete + 1 { return 7 }
        if !bytes_reserve(observed, complete + 1) { return 1 }
        if !bytes_push(observed, u8(*value - 1)) { return 2 }
        emit(*value - 1)
        complete += 1
    }
    return 0
}
