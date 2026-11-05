extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
fn populated(code:i32)->Store<P> contract{result(create)}{let cells:Store<P>=store_new();store_reserve(cells,1);store_push(cells,P{owner:R{code:code},n:42});return give cells}
enum E{Left(Store<P>),Right(Store<P>)}
fn reset(target:&mut E){*target=E.Right(populated(66))}
fn work()->i32{var value=E.Left(populated(65));reset(&mut value);match value{E.Left(cells)=>{let current=store_at(cells,0);return current.n},E.Right(cells)=>{let current=store_at(cells,0);return current.n}}}
fn main()->i32{let seen=work();unsafe{putchar(48+seen/10);putchar(48+seen%10);putchar(10)}return 0}
