// RX10: trait 实现可显式重复与接口一致的 receiver。
struct Point { x: i32 }
trait Measure {
    fn length(self: &Self) -> usize;
}
impl Measure for Point {
    fn length(self: &Self) -> usize => 1
}
