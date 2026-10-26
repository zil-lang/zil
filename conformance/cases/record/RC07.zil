struct Point { x: i32; y: i32 }
fn bad() -> Point { return Point { x: 1, y: 2, z: 3 } }
