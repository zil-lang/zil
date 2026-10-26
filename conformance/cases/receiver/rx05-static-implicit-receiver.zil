// RX05: 静态关联函数使用隐式成员时拒绝，不因正文自动获得接收者。
struct Point { x: i32 }
impl Point {
    fn invalid() -> i32 => .x
}
