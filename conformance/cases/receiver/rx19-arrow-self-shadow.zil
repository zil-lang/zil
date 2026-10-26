// RX19: 禁止箭头闭包参数遮蔽保留接收者名字 self。
struct Point { x: i32 }
impl Point(self: &Self) {
    fn invalid() -> i32 {
        let f = (self: i32) => 0
        return 0
    }
}
