struct Point { x: i32; y: i32 }
fn main() -> i32 {
    let p = Point { x: 1, y: 2 }
    let q: Point = { ...p, x: 4 }
    return p.x * 10 + q.x
}
