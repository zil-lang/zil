// RX11: trait 的共享接收者不得在实现中改成更强的独占接收者。
struct Point { x: i32 }
trait Measure {
    fn length(self: &Self) -> usize;
}
impl Measure for Point {
    fn length(self: &mut Self) -> usize => 1
}
