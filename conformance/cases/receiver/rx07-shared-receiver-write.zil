// RX07: 共享地址接收者不能因省写 self 而获得写权限。
struct Point { x: i32 }
impl Point(self: &Self) {
    fn invalid() { .x = 1 }
}
