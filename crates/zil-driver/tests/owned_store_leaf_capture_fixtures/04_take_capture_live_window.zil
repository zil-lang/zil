
extern "C" fn putchar(n: i32) -> i32;
resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }
package Item { manages ticket: Ticket; value: i32 }
package Bag { manages items: Store<Item>; manages side: Ticket }
 fn make() -> impl FnOnce() -> i32 { 
    let items: Store<Item> = store_new()
    store_reserve(items, 1)
    store_push(items, Item { ticket: Ticket { code: 65 }, value: 42 })

            return () => {
                let view = store_at(items, 0)
                let answer = view.value
                let taken = give items
                return i32(store_len(taken)) + view.value - 42
            }
        }
fn main() -> i32 {
    let callback = make()
    let value = callback()
    unsafe { putchar(90) }
    return value - 1
}
