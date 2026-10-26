// RX22: 实例方法成员不能直接作为已绑定回调取值。
struct Point { x: i32 }
impl Point(self: &Self) {
    fn read() -> i32 => .x
}
fn invalid(p: &Point) {
    let read = p.read
}
