trait Read<T> { fn read(self: &Self) -> T; }
struct Point { value: i32 }
impl Read<i32> for Point { fn read() -> i32 { return .value } }
fn main() -> i32 {
    let value = Point { value: 7 }
    return value.read() - 7
}
