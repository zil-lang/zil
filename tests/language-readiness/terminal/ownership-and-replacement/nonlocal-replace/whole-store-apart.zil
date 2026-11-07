extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
fn populated(code:i32)->Store<P> contract{result(create)}{let cells:Store<P>=store_new();store_reserve(cells,1);store_push(cells,P{owner:R{code:code},n:42});return give cells}
fn work()->i32{var cells=populated(65);let other=populated(67);let preserved=store_at(other,0).owner;cells=populated(66);let next=store_at_mut(cells,0);let current=next.owner;return next.n+current.code+preserved.code-133}
fn main()->i32{let seen=work();unsafe{putchar(48+seen/10);putchar(48+seen%10);putchar(10)}return 0}
