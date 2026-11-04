trait Read { type Item: Copy; }
struct Box<T> {}
impl<T> Read for Box<T> { type Item = T; }
fn main() {}
