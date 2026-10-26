fn main() -> i32 {
    var base = 40
    let add = move (x) => base + x
    base = 0
    return add(2)
}
