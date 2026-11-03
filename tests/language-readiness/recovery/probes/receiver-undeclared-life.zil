struct Point { value: i32 }
impl Point {
    fn read(self: &'a Self) -> &'a i32 { return &self.value }
}
fn main() -> i32 {
    let value = Point { value: 7 }
    return *value.read() - 7
}
