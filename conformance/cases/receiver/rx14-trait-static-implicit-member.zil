// RX14: trait 静态函数中使用隐式成员时拒绝。
struct Point { x: i32 }
trait Zero {
    fn zero() -> i32;
}
impl Zero for Point {
    fn zero() -> i32 => .x
}
