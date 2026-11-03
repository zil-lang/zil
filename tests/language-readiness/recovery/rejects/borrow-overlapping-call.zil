fn read_after_write(read: &i32, write: &mut i32) -> i32 {
    *write = 9
    return *read
}
fn main() -> i32 {
    var value = 7
    return read_after_write(&value, &mut value) - 9
}
