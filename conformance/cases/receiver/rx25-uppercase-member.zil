// RX25: 大写 Center 若确为 receiver 字段，仍可用隐式成员访问。
struct Holder { Center: i32 }
impl Holder(self: &Self) {
    fn center() -> i32 => .Center
}
