fn wide() -> i64 {
    let base: i64 = 2147483648
    let add = move (x) => x + base
    return add(2)
}

fn main() -> i32 {
    let identity = (x) => x
    let invert = (flag) => !flag
    if invert(false) {
        return identity(42)
    } else {
        return identity(0)
    }
}
