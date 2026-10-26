struct Point { x: i32; y: i32 }
struct Other { x: i32; y: i32 }
fn bad(p: Point) -> Other { return Other { ...p } }
