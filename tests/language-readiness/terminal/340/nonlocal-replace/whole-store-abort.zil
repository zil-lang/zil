extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
fn populated(code:i32)->Store<P> contract{result(create)}{let cells:Store<P>=store_new();store_reserve(cells,1);store_push(cells,P{owner:R{code:code},n:42});return give cells}
enum Fault{Bad}
fn fail()->Fault!i32{return Fault.Bad()}
fn stage(give prepared:Store<P>,n:i32)->Store<P> contract{result(give)}{return give prepared}
fn attempt()->Fault!i32{var cells=populated(65);cells=stage(populated(66),fail()?);let current=store_at(cells,0);return current.n}
fn work()->i32{match attempt(){.ok(n)=>{return n},.err(_)=>{return 42}}}
fn main()->i32{let seen=work();unsafe{putchar(48+seen/10);putchar(48+seen%10);putchar(10)}return 0}
