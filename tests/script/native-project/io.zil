extern "C" fn getchar() -> i32;
extern "C" fn putchar(value: i32) -> i32;
pub fn read() -> i32 { unsafe { return getchar() } }
pub fn emit(code: i32) { unsafe { putchar(code) } }
