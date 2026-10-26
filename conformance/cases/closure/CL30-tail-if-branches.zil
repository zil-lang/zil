fn check() -> i32 {
    let absolute = move (value: i32) => {
        if value > 0 { value } else { 0 - value }
    }
    return absolute(-7)
}
