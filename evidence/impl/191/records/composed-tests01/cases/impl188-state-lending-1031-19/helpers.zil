pub fn read<F: Fn() -> &[u8]>(callback: &F) -> &[u8] {
    return callback()
}

pub fn peek<F: Fn() -> &[u8]>(callback: &F) -> &[u8] {
    return read(callback)
}

pub fn append_leaf<F: FnMut(u8) -> &[u8]>(callback: &mut F, value: u8) -> &[u8] {
    return callback(value)
}

pub fn append<F: FnMut(u8) -> &[u8]>(callback: &mut F, value: u8) -> &[u8] {
    return append_leaf(callback, value)
}
extern "C" fn putchar(value: i32) -> i32;
pub fn emit(value: i32) { unsafe { putchar(value) } }
