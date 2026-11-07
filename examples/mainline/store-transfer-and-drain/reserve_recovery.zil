import storage
import work

fn main() -> void!i32 {
    let value = storage.empty()
    work.fill(value, 2)
    if store_len(value) != 2 { return () }
    let before = store_capacity(value)
    let grown = store_reserve(value, 6)
    if !grown && (store_len(value) != 2 || store_capacity(value) != before) { return () }
    if grown && store_capacity(value) != 6 { return () }
    return work.drain(value)? - 131
}
