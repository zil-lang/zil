// RX08: 没有组默认时仍可显式声明接收者并使用隐式成员。
struct Point { x: i32 }
impl Point {
    fn read(self: &Self) -> i32 => .x
}
