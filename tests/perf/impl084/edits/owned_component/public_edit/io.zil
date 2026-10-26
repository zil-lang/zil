extern "C" fn getchar() -> i32;
extern "C" fn putchar(value: i32) -> i32;
pub fn read() -> i32 { unsafe { return getchar() } }
pub fn emit(value: i32) { unsafe { putchar(value) } }
fn digits(value: i32) {
    if value <= -10 || value >= 10 { digits(value / 10) }
    var digit = value % 10
    if digit < 0 { digit = -digit }
    emit(49 + digit)
}
pub fn number(value: i32, offset: i32) {
    if value < 0 { emit(45) }
    digits(value + offset)
}
