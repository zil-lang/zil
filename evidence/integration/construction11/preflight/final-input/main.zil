import rules
import data
extern "C" fn putchar(code: i32) -> i32;
resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }
package Leaf { manages ticket: Ticket; value: data.Counter }
package Item { child: Leaf }
fn main() -> i32 {
    let item = Item { child: Leaf { ticket: Ticket { code: 65 }, value: data.make(7) } }
    let callback = () => rules.read(&item.child.value)
    let moved = item
    let value = callback() + callback()
    if value != 14 { return 1 }
    return 0
}
