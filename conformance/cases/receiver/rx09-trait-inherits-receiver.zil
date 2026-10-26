// RX09: trait 实现省略 receiver 时从既定方法签名补入。
struct Point { x: i32 }
trait Measure {
    fn length(self: &Self) -> usize;
}
impl Measure for Point {
    fn length() -> usize => 1
}
