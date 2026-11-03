fn select(left: &i32, right: &i32, first: bool) -> &i32 {
    if first { return left }
    return right
}
fn main() -> i32 {
    let left = 3
    let right = 4
    return *select(&left, &right, false) - 4
}
