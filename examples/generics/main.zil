fn identity<T>(value: T) -> T => value
fn choose<T>(first: T, second: T, take_first: bool) -> T {
    if take_first { return first }
    return second
}
fn relay<T>(value: T, count: i32) -> T {
    if count == 0 { return value }
    return relay::<T>(value, count - 1)
}
pub fn answer() -> i64 {
    let left: i64 = 4000
    let right: i64 = 23
    let chosen = choose(left, right, true)
    return relay(identity(chosen), 4) + identity::<i64>(23)
}
fn main() -> i32 {
    if answer() == 4023 { return 0 }
    return 1
}
