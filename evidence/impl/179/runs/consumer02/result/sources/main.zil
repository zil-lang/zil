import model
import callbacks
import collection

extern "C" fn getchar() -> i32;

package Batch { manages items: Store<model.Item>; manages bytes: ByteBuffer }
package State<F> {
    manages items: Store<model.Item>
    callback: F
    hits: i32
    manages bytes: ByteBuffer
}
fn batch() -> Batch => Batch { items: collection.empty(), bytes: bytes_new() }
fn pack<F: FnMut() -> i32>(callback: F) -> State<F> {
    let prepared = batch()
    return State::<F> { items: give prepared.items, callback: callback, hits: 0, bytes: give prepared.bytes }
}

fn main() -> i32 {
    let count = getchar() - 48
    if count < 1 || count > 2 { return 9 }
    var current = pack(callbacks.make(65))
    let callback = &mut current.callback
    if !collection.reserve(current.items, i64(count)) { return callback() + 20 }
    var i = 0
    while i < count {
        collection.append(current.items, 66 + i)
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
