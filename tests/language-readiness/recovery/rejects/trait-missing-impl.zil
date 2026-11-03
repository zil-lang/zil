trait Read { fn read(self: &Self) -> i32; }
struct Point { value: i32 }
fn read<T: Read>(value: &T) -> i32 { return value.read() }
fn main() -> i32 {
    let value = Point { value: 7 }
    return read(&value) - 7
}
