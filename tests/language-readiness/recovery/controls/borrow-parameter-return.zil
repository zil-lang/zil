fn identity(value: &i32) -> &i32 { return value }
fn main() -> i32 {
    let value = 7
    return *identity(&value) - 7
}
