fn apply<F: for<life a> Fn(&'a i32) -> i32>(read: F) -> i32 {
    let value = 7
    return read(&value)
}
fn main() -> i32 { return apply((value: &i32) -> i32 => *value) - 7 }
