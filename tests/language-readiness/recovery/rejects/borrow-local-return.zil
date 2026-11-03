fn escape() -> &i32 {
    let value = 7
    return &value
}
fn main() -> i32 { return *escape() - 7 }
