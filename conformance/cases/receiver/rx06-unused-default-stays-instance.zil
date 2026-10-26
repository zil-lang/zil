// RX06: 有组默认的方法即使不使用 self 也保持实例签名。
struct Point { x: i32 }
impl Point(self: &Self) {
    fn constant() -> i32 => 7
}
fn read(p: &Point) -> i32 => p.constant()
