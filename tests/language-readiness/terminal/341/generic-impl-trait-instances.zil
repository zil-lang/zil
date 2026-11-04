trait Read<Tag> { fn read(self: &Self) -> i32; }
struct Box<T: Copy> { value: T }
impl<T: Copy> Read<i32> for Box<T> { fn read() -> i32 { return 7 } }
fn consume<T: Read<i32>>(p: &T) -> i32 { return p.read() }
fn main() -> i32 {
    let p = Box::<i32> { value: 3 };
    let q = Box::<bool> { value: true };
    return consume(&p) + consume(&q) - 14
}
