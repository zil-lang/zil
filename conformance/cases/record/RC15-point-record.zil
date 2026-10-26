struct Point { x: i32; y: i32 }
fn make() -> Point => .{ x: 20, y: 22 }
fn check() -> i32 { return make().x + make().y }
