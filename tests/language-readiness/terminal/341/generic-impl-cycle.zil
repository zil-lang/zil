trait Grow {}
struct Box<T: Copy> { value: T }
impl<T: Copy> Grow for Box<T> where Box<T>: Grow {}
fn consume<T: Grow>(p: &T) {}
fn main() { let p = Box::<i32> { value: 3 }; consume(&p) }
