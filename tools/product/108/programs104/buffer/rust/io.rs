unsafe extern "C" {
    fn getchar() -> i32;
    fn putchar(value: i32) -> i32;
}
pub fn read_byte() -> i32 { unsafe { getchar() } }
pub fn write_byte(value: u8) { unsafe { putchar(i32::from(value)); } }
