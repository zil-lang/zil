struct Pair<T> { first: T; second: T; }
fn first<T>(pair: Pair<T>) -> T => pair.first
fn identity<T>(value: T) -> T => value
fn relay<T>(value: T, count: i32) -> T {
    if count == 0 { return value }
    return relay::<T>(value, count - 1)
}
fn triangular(n: i64) -> i64 {
    var total: i64 = 0
    for i in 0..n { total = total + i + 1 }
    return total
}
fn result() -> i64 {
    let pair = Pair::<i64> { first: #(triangular(10)), second: 7 }
    let copy = identity(pair)
    let selected = first(relay(copy, 3))
    return selected * 100 + pair.second
}
fn main() -> i32 { if result() == 5507 { return 0 } return 1 }
