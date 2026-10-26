extern "C" fn getchar() -> i32;
extern "C" fn putchar(value: i32) -> i32;
pub fn read_byte() -> i32 { unsafe { return getchar() } }
pub fn write_byte(value: u8) { unsafe { putchar(i32(value)) } }
