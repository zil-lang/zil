package Parent { manages values:Store<i32> }
fn result()->i32 {
    let outer:Store<Parent>=store_new()
    return i32(store_len(outer))
}
