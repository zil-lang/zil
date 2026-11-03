fn read(value: &i32) -> i32 { return *value }
fn apply(read: fn(&i32) -> i32, value: &i32) -> i32 { return read(value) }
fn main() -> i32 {
    let value = 7
    return apply(read, &value) - 7
}
