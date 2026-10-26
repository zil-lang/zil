extern "C" fn getchar() -> i32;
extern "C" fn putchar(value: i32) -> i32;

pub fn read() -> i32 {
    unsafe { return getchar() }
}

pub fn write(value: i32) -> i32 {
    unsafe { return putchar(value) }
}

fn digits(value: i32) -> bool {
    if value <= -10 || value >= 10 {
        if !digits(value / 10) { return false }
    }
    var digit = value % 10
    if digit < 0 { digit = -digit }
    return write(48 + digit) >= 0
}

pub fn number(value: i32) -> bool {
    if value < 0 {
        if write(45) < 0 { return false }
    }
    return digits(value)
}
