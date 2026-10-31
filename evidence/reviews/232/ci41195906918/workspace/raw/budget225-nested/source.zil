extern "C" fn putchar(n: i32) -> i32;
resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }
package Leaf { manages ticket: Ticket; value: i32 }
package Item { first: Leaf; second: Leaf; tag: i32 }
fn make() -> impl Fn() -> i32 {
    let items: Store<Item> = store_new()
    store_reserve(items, 2)
    store_push(items, Item { first: Leaf { ticket: Ticket { code: 65 }, value: 7 }, second: Leaf { ticket: Ticket { code: 66 }, value: 9 }, tag: 3 })
    return move () => { return i32(store_len(items)) }
}
fn main() -> i32 { let callback = make(); return callback() - 1 }
