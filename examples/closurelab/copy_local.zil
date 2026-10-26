fn main() -> i32 {
    var enabled = true
    let choose = move (x) => {
        if enabled {
            return x
        } else {
            return x + 1
        }
    }
    enabled = false
    var copy = choose
    copy = choose
    return copy(42)
}
