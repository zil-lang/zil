extern "C" fn putchar(value: i32) -> i32;
resource Marker { code: i32 }
impl Drop for Marker { fn drop(self: Self) { unsafe { putchar(.code) } } }
package Item { pad: u8; manages marker: Marker; wide: i64; tail: [u16; 3] }
fn make(code: i32) -> Item {
    return Item { pad: u8(7), marker: Marker { code: code }, wide: i64(code),
                  tail: [u16(11), u16(12), u16(13)] }
}
fn main() -> void!i32 {
    let items: Store<Item> = store_new()
    if !store_reserve(items, 2) { return () }
    store_push(items, make(65))
    store_push(items, make(66))
    // Full insertion returns the entire element; discarded error drops C once.
    store_push(items, make(67))
    let item = store_at(items, 1)
    if item.pad != u8(7) || item.wide != 66 || item.tail[2] != u16(13) { return 20 }
    if store_reserve(items, -1) { return 21 }
    if store_reserve(items, 9223372036854775807) { return 22 }
    let grown = store_reserve(items, 4)
    if store_len(items) != 2 { return 23 }
    if !grown && store_capacity(items) != 2 { return 24 }
    let after = store_at(items, 1)
    if after.wide != 66 || after.tail[0] != u16(11) { return 25 }
    let last = store_pop(items)?
    if last.wide != 66 { return 26 }
    return 0
}
