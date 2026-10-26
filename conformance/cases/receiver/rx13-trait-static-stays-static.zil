// RX13: trait 静态函数在实现中不会自动补入 self。
struct Point { x: i32 }
trait Zero {
    fn zero() -> i32;
}
impl Zero for Point {
    fn zero() -> i32 => 0
}
