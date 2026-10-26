fn fib(n: i64) -> i64 {
    if n < 2 { return n }
    return fib(n - 1) + fib(n - 2)
}
fn sum_even(n: i64) -> i64 {
    var sum: i64 = 0
    for i in 0..n {
        if i % 2 != 0 { continue }
        sum += i
    }
    return sum
}
fn gcd(a: i64, b: i64) -> i64 {
    var x = a
    var y = b
    while y != 0 {
        let next = x % y
        x = y
        y = next
    }
    return x
}
pub fn answer() -> i64 {
    let a = #(fib(12))
    let b = #(sum_even(11))
    let c = #(gcd(84, 30))
    return a + b + c
}
fn main() -> i32 {
    if answer() == 180 { return 0 }
    return 1
}
