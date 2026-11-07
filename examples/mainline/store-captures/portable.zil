pub struct Tag { pub sequence: i64; pub valid: bool }

extern "C" fn putchar(value: i32) -> i32;
extern "C" fn getchar() -> i32;

pub fn emit(value: i32) { unsafe { putchar(value) } }
pub fn read() -> i32 { unsafe { return getchar() } }
