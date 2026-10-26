import state

fn direct() -> impl FnMut(u8) -> i64 {
    let data = bytes_new()
    bytes_reserve(data, 8)
    return move(value) => {
        bytes_push(data, value)
        let values = bytes_view(data)
        return i64(values[0]) + values.len()
    }
}

fn main() -> i32 {
    var update = state.make()
    if update(7u8) != 8 { return 1 }
    if update(9u8) != 18 { return 2 }
    if update(0u8) != 0 { return 3 }
    if update(3u8) != 4 { return 4 }
    var saved = direct()
    if saved(7u8) != 8 { return 5 }
    if saved(9u8) != 9 { return 6 }
    return 0
}
