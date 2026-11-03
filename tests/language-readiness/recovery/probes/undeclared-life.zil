fn identity(value: &'missing i32) -> &'missing i32 { return value }
fn main() -> i32 {
    let value = 7
    return *identity(&value) - 7
}
