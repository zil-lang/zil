struct Point { x: i32; y: i32 }
fn make() -> Point => ({ x: 2, y: 3 })
fn main() -> i32 { return make().x }
