struct Point { x: i32; y: i32 }
fn bad(p: Point) -> Point { return Point { x: 2, ...p } }
