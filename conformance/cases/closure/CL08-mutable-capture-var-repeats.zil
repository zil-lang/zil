fn check() -> i64 {
    var count: i64 = 0
    var next = () => {
        count += 1
        return count
    }
    let first = next()
    let second = next()
    return first * 100 + second * 10 + count
}
