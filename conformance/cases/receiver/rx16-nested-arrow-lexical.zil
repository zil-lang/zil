// RX16: 嵌套箭头闭包继续引用同一个词法接收者。
struct Point { x: i32 }
impl Point(self: &Self) {
    fn nested() -> i32 {
        let outer = () => {
            let inner = () => .x
            return inner()
        }
        return outer()
    }
}
