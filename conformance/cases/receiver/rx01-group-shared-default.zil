// RX01: 组默认补入共享地址接收者，隐式成员绑定同一 self。
struct Point { x: i32, y: i32 }
impl Point(self: &Self) {
    fn sum() -> i32 => .x + .y
}
