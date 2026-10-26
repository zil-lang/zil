// RX20: 块内普通具名 fn 不属于语法，不能将其隐式改成捕获闭包。
struct Point { x: i32 }
impl Point(self: &Self) {
    fn invalid() -> i32 {
        fn inner() -> i32 => 1
        return 0
    }
}
