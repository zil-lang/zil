fn select(left: &'left i32, right: &'right i32) -> &'left i32 { return right }
fn main() -> i32 {
    let left = 3
    let right = 4
    return *select(&left, &right) - 4
}
