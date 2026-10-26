// RX02: 方法显式独占地址接收者覆盖组默认，可在可写实例上修改字段。
struct Point { x: i32, y: i32 }
impl Point(self: &Self) {
    fn shift(self: &mut Self, dx: i32) { .x += dx }
}
fn exercise() -> i32 {
    var p = Point { x: 1, y: 2 }
    p.shift(3)
    return p.x
}
