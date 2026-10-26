// RX21: Type.method 是仍带 receiver 首参数的无绑定函数值。
struct Point { x: i32 }
impl Point(self: &Self) {
    fn read() -> i32 => .x
}
fn call_unbound(p: &Point) -> i32 {
    let read: fn(&Point) -> i32 = Point.read
    return read(p)
}
