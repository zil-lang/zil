// RX15: 箭头闭包中的 .x 与 self.x 产生等价的词法绑定与捕获。
struct Point { x: i32 }
impl Point(self: &Self) {
    fn implicit() -> i32 {
        let read = () => .x
        return read()
    }
    fn explicit() -> i32 {
        let read = () => self.x
        return read()
    }
}
