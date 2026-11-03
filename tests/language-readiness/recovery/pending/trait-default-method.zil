trait Read { fn read(self: &Self) -> i32 { return 7 } }
struct Point { value: i32 }
impl Read for Point {}
fn main() -> i32 {
    let value = Point { value: 7 }
    return value.read() - 7
}
