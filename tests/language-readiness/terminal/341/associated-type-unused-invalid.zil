trait Read { type Item: Copy; }
struct P {}
impl Read for P { type Item = i32; }
fn bad<T: Read>(value: T.Item) -> T.Item { return value + 1 }
fn main() {}
