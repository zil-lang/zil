// RX23: 隐式 receiver 的方法成员取值与 obj.method 同样拒绝。
struct Point { x: i32 }
impl Point(self: &Self) {
    fn read() -> i32 => .x
    fn invalid() {
        let read = .read
    }
}
