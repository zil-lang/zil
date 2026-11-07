extern "C" fn putchar(n: i32) -> i32;
resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }
package Leaf { manages ticket: Ticket; manages bytes: ByteBuffer; value: i32 }
package Item { first: Leaf; second: Leaf; tag: i32 }
fn main() -> void!i32 {
    let items: Store<Item> = store_new()
    if !store_reserve(items, 1) { return () }
    let aa = Ticket { code: 65 }
    let a = bytes_new()
    if !bytes_reserve(a, 7) { return () }
    let bb = Ticket { code: 66 }
    let b = bytes_new()
    if !bytes_reserve(b, 11) { return () }
    store_push(items, Item { first: Leaf { ticket: give aa, bytes: give a, value: 7 }, second: Leaf { ticket: give bb, bytes: give b, value: 9 }, tag: 3 })
    var callback = move () => {
        if !store_reserve(items, 2) { return 1 }
        return 0
    }
    return callback()
}
