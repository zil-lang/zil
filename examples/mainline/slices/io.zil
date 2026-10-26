extern "C" fn getchar() -> i32;
extern "C" fn putchar(value: i32) -> i32;
pub fn read() -> i32 { unsafe { return getchar() } }
pub fn emit(bytes: &[u8]) -> i32 {
    var index: i64 = 0
    while index < bytes.len() {
        var result = 0
        unsafe { result = putchar(i32(bytes[index])) }
        if result < 0 { return 1 }
        index += 1
    }
    return 0
}
