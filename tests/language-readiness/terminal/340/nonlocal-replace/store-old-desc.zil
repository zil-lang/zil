extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
fn reset(target:&mut P){*target=P{owner:R{code:66},n:42}}
fn empty()->Store<P> contract{result(create)}{return store_new()}
fn work()->i32{let cells=empty();store_reserve(cells,2);store_push(cells,P{owner:R{code:65},n:0});let view=store_at(cells,0);let old=view.owner;let target=store_at_mut(cells,0);*target=P{owner:R{code:66},n:42};let next=store_at_mut(cells,0);let current=next.owner;return old.code}
fn main()->i32{let seen=work();unsafe{putchar(48+seen/10);putchar(48+seen%10);putchar(10)}return 0}
