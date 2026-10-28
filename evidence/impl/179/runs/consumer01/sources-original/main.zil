import model
import callbacks
import state

extern "C" fn getchar() -> i32;

fn main() -> i32 {
    let count = getchar() - 48
    if count < 1 || count > 2 { return 9 }
    var current = state.pack(callbacks.make(65))
    let callback = &mut current.callback
    if !store_reserve(current.items, i64(count)) { return callback() + 20 }
    var i = 0
    while i < count {
        store_push(current.items, model.item(66 + i))
        i += 1
    }
    let selected = store_at(current.items, i64(count - 1))
    current.hits = selected.code
    current.hits += callback()
    current.hits += callback()
    if !bytes_reserve(current.bytes, 4) { return callback() + 19 }
    bytes_push(current.bytes, 9u8)
    let bytes = bytes_view(current.bytes)
    let child = store_at(current.items, i64(count - 1))
    current.hits += callback()
    let total = current.hits + child.code + i32(bytes.len())
    return total - (2 * (65 + count) + 7)
}
