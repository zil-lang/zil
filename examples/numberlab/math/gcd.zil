pub fn gcd(left: i32, right: i32) -> i32 {
    var a = left
    var b = right
    while b != 0 {
        let remainder = a % b
        a = b
        b = remainder
    }
    return a
}
