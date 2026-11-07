pub fn append<F: FnMut(&mut i32) -> i32>(callback: &mut F, value: &mut i32) -> i32 {
    return callback(value)
}

extern "C" fn putchar(value: i32) -> i32;
pub fn emit(value: i32) { unsafe { putchar(value) } }
