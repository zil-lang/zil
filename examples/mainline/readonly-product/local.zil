extern "C" fn putchar(code: i32) -> i32;
resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }
package Leaf { manages ticket: Ticket; value: i32 }
package Item { child: Leaf; tag: i32 }
fn main() -> i32 {
    let item = Item { child: Leaf { ticket: Ticket { code: 65 }, value: 7 }, tag: 3 }
    let callback = () => { return item.child.value + item.tag }
    let value = callback() + callback()
    let moved = item
    return value - 20
}
