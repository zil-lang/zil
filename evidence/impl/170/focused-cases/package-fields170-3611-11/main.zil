
extern "C" fn putchar(x:i32)->i32;
resource R { code:i32 }
impl Drop for R { fn drop(self:Self) { unsafe { putchar(.code) } } }
package Item { manages item:R; n:i32 }
package State { manages tag:R; items:[Item;2]; n:i32 }
fn item(n:i32)->Item => Item { item:R{code:n}, n:n }
fn state(tag:i32,a:i32,b:i32)->State => State{tag:R{code:tag},items:[item(a),item(b)],n:4}
 fn main()->i32{var s=state(90,65,66);let items=s.items;s.items=items;s.items=items;return 0}