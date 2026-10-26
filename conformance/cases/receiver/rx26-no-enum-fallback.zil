// RX26: 期望枚举类型不能让缺失 receiver 成员 .Center 回退成枚举标签。
enum Alignment { Center, Start }
struct Point { x: i32 }
impl Point(self: &Self) {
    fn invalid() -> Alignment => .Center
}
