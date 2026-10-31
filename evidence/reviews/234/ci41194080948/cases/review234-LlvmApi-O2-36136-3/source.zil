
extern "C" fn getchar() -> i32;
extern "C" fn putchar(n: i32) -> i32;
fn choose32(x: i32) -> i32 {
    if x < 0 { return -2147483648 }
    if x == 0 { return -257 }
    if x == 1 { return -256 }
    if x == 2 { return -1 }
    if x == 3 { return 2147483647 }
    if x == 4 { return -2147483392 }
    var n = x
    while n > 4 { n = n - 1 }
    return -2
}
fn choose64(x: i32) -> i64 {
    if x < 0 { return -9223372036854775808 }
    if x == 0 { return -4294967297 }
    if x == 1 { return -4294967296 }
    if x == 2 { return -1 }
    if x == 3 { return 9223372036854775807 }
    if x == 4 { return -9223372036854775552 }
    return -2
}
fn emit32(n: i32) {
    unsafe {
        putchar(n & 255)
        putchar((n >> 8) & 255)
        putchar((n >> 16) & 255)
        putchar((n >> 24) & 255)
    }
}
fn emit64(n: i64) {
    unsafe {
        putchar(i32(n & 255))
        putchar(i32((n >> 8) & 255))
        putchar(i32((n >> 16) & 255))
        putchar(i32((n >> 24) & 255))
        putchar(i32((n >> 32) & 255))
        putchar(i32((n >> 40) & 255))
        putchar(i32((n >> 48) & 255))
        putchar(i32((n >> 56) & 255))
    }
}
fn main() -> i32 {
    var x = 0
    unsafe { x = getchar() }
    emit32(choose32(x))
    emit64(choose64(x))
    return 0
}
