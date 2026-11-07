pub fn read<F: Fn(&[u8]) -> i32>(callback: &F, values: &[u8]) -> i32 {
    return callback(values)
}

pub fn edit<F: FnMut(&mut [u8]) -> i32>(callback: &mut F, values: &mut [u8]) -> i32 {
    return callback(values)
}

pub fn consume<F: FnOnce(&mut [u8]) -> i32>(callback: F, values: &mut [u8]) -> i32 {
    return callback(values)
}

pub fn extra<F: Fn() -> i32>(callback: &F, values: &[u8]) -> i32 {
    return callback() + i32(values[0])
}

extern "C" fn putchar(value: i32) -> i32;
pub fn emit(value: i32) { unsafe { putchar(value) } }
