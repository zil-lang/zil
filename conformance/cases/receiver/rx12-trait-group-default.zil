// RX12: trait impl 禁止附带组默认接收者。
struct Point { x: i32 }
trait Measure {
    fn length(self: &Self) -> usize;
}
impl Measure for Point(self: &Self) {
    fn length() -> usize => 1
}
