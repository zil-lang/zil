trait Read { fn read(self: &Self) -> i32; }
struct Box<T: Copy> { value: T }
impl<T: Copy> Read for Box<T> {
    fn read(self: &Self) -> i32 { return self.value + 1 }
}
fn main() {}
