// RX03: 方法显式按值接收者覆盖组默认，其类别不由正文决定。
struct Point { x: i32, y: i32 }
impl Point(self: &Self) {
    fn sum_value(self: Self) -> i32 => .x + .y
}
