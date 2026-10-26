// RX24: 显式箭头闭包可构造方法回调，捕获与依赖经过统一检查。
struct Point { x: i32 }
impl Point(self: &Self) {
    fn read() -> i32 => .x
}
fn call_callback(p: &Point) -> i32 {
    let read = () => p.read()
    return read()
}
