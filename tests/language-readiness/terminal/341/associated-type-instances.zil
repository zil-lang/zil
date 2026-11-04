trait Read<Tag> {
    type Item: Copy;
    fn read(self: &Self) -> Self.Item;
}
struct Box<T: Copy> { value: T }
impl<T: Copy> Read<i32> for Box<T> {
    type Item = T;
    fn read() -> Self.Item { return .value }
}
impl<T: Copy> Read<bool> for Box<T> {
    type Item = bool;
    fn read() -> Self.Item { return true }
}
fn number<T: Read<i32>>(value: &T) -> T.Item { return value.read() }
fn flag<T: Read<bool>>(value: &T) -> T.Item { return value.read() }
fn main() -> i32 {
    let value = Box::<i32> { value: 7 };
    if flag::<Box<i32>>(&value) { return number::<Box<i32>>(&value) - 7 }
    return 1
}
