// RX28: move 闭包使用地址接收者时仍保留真实地址依赖，不获得目标责任。
struct Point { x: i32 }
impl Point(self: &Self) {
    fn read_with_move() -> i32 {
        let read = move () => .x
        return read()
    }
}
