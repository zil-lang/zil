fn check() -> i32 {
    let offset: i32 = 10
    let add = move x => { x + offset }
    return add(32)
}
