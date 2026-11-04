trait Base { fn base(self: &Self) -> i32; }
trait Read: Copy { fn read(self: &Self) -> i32; }
struct P { n: i32 }
struct Q { n: i32 }
impl Base for P { fn base() -> i32 { return .n } }
impl Base for Q { fn base() -> i32 { return .n } }
struct Box<T: Copy> { value: T }
impl<T: Copy> Read for Box<T> where T: Base {
    fn read(self: &Self) -> i32 { return self.value.base() }
}
fn consume<T: Read>(p: &T) -> i32 { return p.read() }
fn main() -> i32 {
    let p = Box::<P> { value: P { n: 7 } };
    let q = Box::<Q> { value: Q { n: 8 } };
    return consume(&p) + q.read() - 15
}
