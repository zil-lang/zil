struct Point { x: i32; y: i32 }
fn bad() -> Point { return { x: 1 } }
