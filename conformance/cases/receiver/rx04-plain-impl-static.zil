// RX04: 普通 impl 的无 self 函数是静态关联函数。
struct Point { x: i32, y: i32 }
impl Point {
    fn origin() -> Self => Point { x: 0, y: 0 }
}
fn make() -> Point => Point.origin()
