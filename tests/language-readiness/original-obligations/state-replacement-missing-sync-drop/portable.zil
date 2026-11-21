pub struct Tag { pub sequence: i64; pub valid: bool }
pub resource Guard { pub code: i32 }
impl Drop for Guard { fn drop(self: Self) { emit(.code) } }

extern "C" fn putchar(value: i32) -> i32;
extern "C" fn getchar() -> i32;

pub fn emit(value: i32) { unsafe { putchar(value) } }
pub fn read() -> i32 { unsafe { return getchar() } }

pub fn fail_label() -> void!i32 { emit(69); return () }
pub fn fail_flag() -> void!bool { emit(69); return () }
