pub struct Counter { pub code: i32 }
extern "C" fn putchar(value: i32) -> i32;
pub fn emit(value: i32) { unsafe { putchar(value) } }
