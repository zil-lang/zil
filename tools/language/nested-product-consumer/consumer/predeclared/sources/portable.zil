// 206 consumer draft: not yet a language execution or accepted fixture.
pub resource Marker { pub code: i32 }
impl Drop for Marker { fn drop(self: Self) { emit(.code) } }
extern "C" fn putchar(value: i32) -> i32;
extern "C" fn getchar() -> i32;
pub fn emit(value: i32) { unsafe { putchar(value) } }
pub fn read() -> i32 { unsafe { return getchar() } }
pub fn fail_tag() -> void!i32 { emit(69); return () }
