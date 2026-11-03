trait Read { fn read(self: &Self) -> i32; }
struct Point<T: Copy> { value: T }
impl<T: Copy> Read for Point<T> { fn read() -> i32 { return 7 } }
fn main() -> i32 {
    let value = Point::<i32> { value: 7 }
    return value.read() - 7
}
