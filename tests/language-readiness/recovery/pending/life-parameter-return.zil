fn identity<life a>(value: &'a i32) -> &'a i32 { return value }
fn main() -> i32 {
    let value = 7
    return *identity(&value) - 7
}
