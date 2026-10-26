// RX27: 真实函数字段可正常取值，不能误判为方法绑定。
struct Handler { callback: fn(i32) -> i32 }
impl Handler(self: &Self) {
    fn apply(value: i32) -> i32 {
        let f = .callback
        return f(value)
    }
}
fn twice(value: i32) -> i32 => value + value
fn exercise() -> i32 {
    let h = Handler { callback: twice }
    return h.apply(3)
}
