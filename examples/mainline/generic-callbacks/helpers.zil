pub struct A { pub n: i32 }
pub struct B { pub n: i32 }
pub struct Tag { pub sequence: i64; pub valid: bool }

pub fn map<T: Copy, F: Fn(T) -> T>(callback: &F, value: T) -> T {
    return callback(value)
}

pub fn keep<T: Copy, F: Fn() -> i32>(callback: &F, value: T) -> T {
    callback()
    return value
}

pub fn stamp<T: Copy, F: FnMut(&mut [u8]) -> i32>(
    callback: &mut F, value: T, bytes: &mut [u8]
) -> (i32, T) {
    let count = callback(bytes)
    return (count, value)
}

pub fn duplicate<T: Copy>(value: T) -> (T, T) { return (value, value) }

extern "C" fn putchar(value: i32) -> i32;
pub fn emit(value: i32) { unsafe { putchar(value) } }
