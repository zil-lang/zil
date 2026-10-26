// RX18: 禁止局部绑定占用保留接收者名字 self。
struct Point { x: i32 }
impl Point(self: &Self) {
    fn invalid() -> i32 {
        let self = 1
        return 0
    }
}
