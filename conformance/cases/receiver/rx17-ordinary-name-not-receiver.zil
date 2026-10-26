// RX17: 同名普通局部或参数不改变隐式成员的 receiver 绑定。
struct Point { x: i32 }
impl Point(self: &Self) {
    fn add_local() -> i32 {
        let x = 10
        let add = (x: i32) => .x + x
        return add(x)
    }
}
