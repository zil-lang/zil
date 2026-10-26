fn sum(n: i32) -> i32 {
    var i = 0
    var total = 0
    while i < n {
        total += i
        i += 1
    }
    return total
}
fn choose(x: i32) -> i32 {
    if x > 0 {
        return x
    } else {
        return 0 - x
    }
}
fn main() -> i32 {
    return sum(10) + choose(-3)
}
