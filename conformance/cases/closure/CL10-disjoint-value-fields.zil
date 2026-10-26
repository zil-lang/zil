struct Pair {
    left: i64
    right: i64
}
fn check() -> i64 {
    var pair = Pair { left: 3, right: 4 }
    let read = () => pair.left
    pair.right = 8
    return read() * 10 + pair.right
}
